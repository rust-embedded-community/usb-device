use core::cmp::min;
use core::mem;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use ::{Result, UsbError};
use utils::AtomicMutex;
use bus::{UsbBusWrapper, UsbBus, PollResult};
use endpoint::{EndpointType, EndpointIn, EndpointOut};
use control;
use class::UsbClass;
pub use device_builder::{UsbDeviceBuilder, UsbVidPid};

/// The global state of the USB device.
///
/// In general class traffic is only possible in the `Configured` state.
#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum UsbDeviceState {
    /// The USB device has just been created or reset.
    Default,

    /// The USB device has received an address from the host.
    Addressed,

    /// The USB device has been configured and is fully functional.
    Configured,

    /// The USB device has been suspended by the host or it has been unplugged from the USB bus.
    Suspend,
}

#[derive(PartialEq, Eq, Debug)]
#[allow(unused)]
enum ControlState {
    Idle,
    DataIn,
    DataInZlp,
    DataInLast,
    StatusOut,
    DataOut,
    StatusIn,
    Error,
}

/// Holds the current state and data buffer for control requests.
struct Control {
    state: ControlState,
    request: Option<control::Request>,
    buf: [u8; 128],
    i: usize,
    len: usize,
    pending_address: u8,
}

const MAX_ENDPOINTS: usize = 16;

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<'a, B: UsbBus + 'a> {
    pub(crate) bus: &'a B,
    control_out: EndpointOut<'a, B>,
    control_in: EndpointIn<'a, B>,

    pub(crate) info: UsbDeviceInfo<'a>,

    class_arr: [&'a (dyn UsbClass + Sync); 8],
    class_count: usize,

    control: AtomicMutex<Control>,
    pub(crate) device_state: AtomicUsize,
    pub(crate) remote_wakeup_enabled: AtomicBool,
    pub(crate) self_powered: AtomicBool,
}

impl<'a, B: UsbBus + 'a> UsbDevice<'a, B> {
    /// Creates a [`UsbDeviceBuilder`] for constructing a new instance.
    pub fn new(bus: &'a UsbBusWrapper<B>, vid_pid: UsbVidPid) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder::new(bus, vid_pid)
    }

    pub(crate) fn build(bus: &'a UsbBusWrapper<B>, classes: &[&'a (dyn UsbClass + Sync)], info: UsbDeviceInfo<'a>)
        -> UsbDevice<'a, B>
    {
        let control_out = bus.alloc(Some(0.into()), EndpointType::Control,
            info.max_packet_size_0 as u16, 0).expect("failed to alloc control endpoint");

        let control_in = bus.alloc(Some(0.into()), EndpointType::Control,
            info.max_packet_size_0 as u16, 0).expect("failed to alloc control endpoint");

        let bus = bus.freeze();

        let mut dev = UsbDevice {
            bus,
            control_out,
            control_in,

            info,

            class_arr: unsafe { mem::uninitialized() },
            class_count: classes.len(),

            control: AtomicMutex::new(Control {
                state: ControlState::Idle,
                request: None,
                buf: [0; 128],
                i: 0,
                len: 0,
                pending_address: 0,
            }),
            device_state: AtomicUsize::new(UsbDeviceState::Default as usize),
            remote_wakeup_enabled: AtomicBool::new(false),
            self_powered: AtomicBool::new(false),
        };

        assert!(classes.len() <= dev.class_arr.len());

        dev.class_arr[..dev.class_count].copy_from_slice(classes);

        {
            let mut control = dev.control.try_lock().unwrap();
            dev.reset(&mut control);
        }

        dev
    }

    pub(crate) fn classes(&self) -> &[&'a (dyn UsbClass + Sync)] {
        &self.class_arr[..self.class_count]
    }

    /// Gets the current state of the device.
    ///
    /// In general class traffic is only possible in the `Configured` state.
    pub fn state(&self) -> UsbDeviceState {
        unsafe { mem::transmute(self.device_state.load(Ordering::SeqCst) as u8) }
    }

    pub(crate) fn set_state(&self, state: UsbDeviceState) {
        self.device_state.store(state as usize, Ordering::SeqCst);
    }

    /// Gets whether host remote wakeup has been enabled by the host.
    pub fn remote_wakeup_enabled(&self) -> bool {
        self.remote_wakeup_enabled.load(Ordering::SeqCst)
    }

    /// Gets whether the device is currently self powered.
    pub fn self_powered(&self) -> bool {
        self.self_powered.load(Ordering::SeqCst)
    }

    /// Sets whether the device is currently self powered.
    pub fn set_self_powered(&self, is_self_powered: bool) {
        self.self_powered.store(is_self_powered, Ordering::SeqCst);
    }

    pub fn force_reset(&self) -> Result<()> {
        self.bus.force_reset()
    }

    /// Polls the [`UsbBus`] for new events and dispatches them accordingly. This should be called
    /// periodically  more often than once every 10 milliseconds to stay USB-compliant, or
    /// from an interrupt handler.
    pub fn poll(&self) {
        let mut guard = self.control.try_lock();

        let control = match guard {
            Some(ref mut c) => c,
            None => { return; } // re-entrant call!
        };

        let pr = self.bus.poll();

        if self.state() == UsbDeviceState::Suspend {
            if !(pr == PollResult::Suspend || pr == PollResult::None) {
                self.bus.resume();
                self.set_state(UsbDeviceState::Default)
            } else {
                return;
            }
        }

        match pr {
            PollResult::None => { }
            PollResult::Reset => self.reset(control),
            PollResult::Data { ep_out, ep_in_complete, ep_setup } => {
                // Combine bit fields for quick tests
                let all = ep_out | ep_in_complete | ep_setup;

                // Pending events for endpoint 0?
                if (all & 1) != 0 {
                    if (ep_setup & 1) != 0 {
                        self.handle_control_setup(control);
                    } else if (ep_out & 1) != 0 {
                        self.handle_control_out(control);
                    }

                    if (ep_in_complete & 1) != 0 {
                        self.handle_control_in_complete(control);
                    }
                }

                // Pending events for other endpoints?
                if (all & !1) != 0 {
                    let mut bit = 2u16;
                    for i in 1..(MAX_ENDPOINTS as u8) {
                        if (ep_setup & bit) != 0 {
                            for cls in self.classes() {
                                cls.endpoint_setup(i);
                            }
                        } else if (ep_out & bit) != 0 {
                            for cls in self.classes() {
                                cls.endpoint_out(i);
                            }
                        }

                        if (ep_in_complete & bit) != 0 {
                            for cls in self.classes() {
                                cls.endpoint_in_complete(i | 0x80);
                            }
                        }

                        bit <<= 1;
                    }
                }
            },
            PollResult::Resume => { }
            PollResult::Suspend => {
                self.bus.suspend();
                self.set_state(UsbDeviceState::Suspend);
            }
        }
    }

    fn reset(&self, control: &mut Control) {
        self.bus.reset();

        self.set_state(UsbDeviceState::Default);
        self.remote_wakeup_enabled.store(false, Ordering::SeqCst);

        control.state = ControlState::Idle;
        control.pending_address = 0;

        for cls in self.classes() {
            cls.reset().unwrap();
        }
    }

    fn handle_control_setup(&self, control: &mut Control) {
        let count = self.control_out.read(&mut control.buf[..]).unwrap();

        let req = match control::Request::parse(&control.buf[0..count]) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                return self.set_control_error(control)
            },
        };

        /*sprintln!("SETUP {:?} {:?} {:?} req:{} val:{} idx:{} len:{} {:?}",
            req.direction, req.request_type, req.recipient,
            req.request, req.value, req.index, req.length,
            control.state);*/

        control.request = Some(req);

        if req.direction == control::Direction::HostToDevice {
            if req.length > 0 {
                if req.length as usize > control.buf.len() {
                    // Transfer length won't fit in buffer
                    return self.set_control_error(control);
                }

                control.i = 0;
                control.len = req.length as usize;
                control.state = ControlState::DataOut;
            } else {
                control.len = 0;
                self.complete_control_out(control);
            }
        } else {
            let mut res = ControlInResult::Ignore;

            for cls in self.classes() {
                res = cls.control_in(&req, &mut control.buf);

                if res != ControlInResult::Ignore {
                    break;
                }
            }

            if res == ControlInResult::Ignore && req.request_type == control::RequestType::Standard {
                res = self.standard_control_in(&req, &mut control.buf);
            }

            if let ControlInResult::Ok(count) = res {
                control.i = 0;
                control.len = min(count, req.length as usize);
                control.state = ControlState::DataIn;

                self.write_control_in_chunk(control);
            } else {
                // Nothing accepted the request or there was an error
                self.set_control_error(control);
            }
        }
    }

    fn handle_control_out(&self, control: &mut Control) {
        match control.state {
            ControlState::DataOut => {
                let i = control.i;
                let count = match self.control_out.read(&mut control.buf[i..]) {
                    Ok(count) => count,
                    Err(_) => {
                        // Failed to read or buffer overflow (overflow is only possible if the host
                        // sends more data than indicated in the SETUP request)
                        return self.set_control_error(control)
                    },
                };

                control.i += count;

                if control.i >= control.len {
                    self.complete_control_out(control);
                }
            },
            ControlState::StatusOut => {
                self.control_out.read(&mut []).unwrap();
                control.state = ControlState::Idle;
            },
            _ => {
                // Discard the packet
                self.control_out.read(&mut control.buf[..]).ok();

                // Unexpected OUT packet
                self.set_control_error(control)
            },
        }
    }

    fn handle_control_in_complete(&self, control: &mut Control) {
        match control.state {
            ControlState::DataIn => {
                self.write_control_in_chunk(control);
            },
            ControlState::DataInZlp => {
                match self.control_in.write(&[]) {
                    Err(UsbError::Busy) => return,
                    Err(err) => panic!("{:?}", err),
                    _ => {},
                };

                control.state = ControlState::DataInLast;
            },
            ControlState::DataInLast => {
                self.control_out.unstall();
                control.state = ControlState::StatusOut;
            }
            ControlState::StatusIn => {
                if control.pending_address != 0 {
                    // SET_ADDRESS is really handled after the status packet has been sent
                    self.bus.set_device_address(control.pending_address);
                    self.set_state(UsbDeviceState::Addressed);
                    control.pending_address = 0;
                }

                control.state = ControlState::Idle;
            },
            _ => {
                // Unexpected IN packet
                self.set_control_error(control);
            }
        };
    }

    fn write_control_in_chunk(&self, control: &mut Control) {
        let count = min(control.len - control.i, self.info.max_packet_size_0 as usize);

        let count = match self.control_in.write(&control.buf[control.i..(control.i+count)]) {
            Err(UsbError::Busy) => return,
            Err(err) => panic!("{:?}", err),
            Ok(c) => c,
        };

        control.i += count;

        if control.i >= control.len {
            control.state = if count == self.info.max_packet_size_0 as usize {
                ControlState::DataInZlp
            } else {
                ControlState::DataInLast
            };
        }
    }

    fn complete_control_out(&self, control: &mut Control) {
        let req = control.request.take().unwrap();

        let mut res = ControlOutResult::Ignore;

        {
            let buf = &control.buf[..control.len];

            for cls in self.classes().iter() {
                res = cls.control_out(&req, buf);

                if res != ControlOutResult::Ignore {
                    break;
                }
            }

            if res == ControlOutResult::Ignore && req.request_type == control::RequestType::Standard {
                res = self.standard_control_out(&req, buf, &mut control.pending_address);
            }
        }

        if res == ControlOutResult::Ok {
            // Send empty packet to indicate success
            self.control_in.write(&[]).ok();
            control.state = ControlState::StatusIn;
        } else {
            // Nothing accepted the request or there was an error
            self.set_control_error(control);
        }
    }

    fn set_control_error(&self, control: &mut Control) {
        control.state = ControlState::Error;
        self.control_out.stall();
        self.control_in.stall();
    }
}

#[derive(Copy, Clone)]
pub(crate) struct UsbDeviceInfo<'a> {
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer: &'a str,
    pub product: &'a str,
    pub serial_number: &'a str,
    pub self_powered: bool,
    pub supports_remote_wakeup: bool,
    pub max_power: u8,
}

/// Result returned by classes for a control OUT transfer.
///
/// Also used internally for non-class requests.
#[derive(Eq, PartialEq, Debug)]
pub enum ControlOutResult {
    /// Ignore the request and pass it to the next class.
    Ignore,

    /// Accept the request.
    Ok,

    /// Report an error to the host.
    Err,
}

/// Result returned by classes for a control IN transfer.
///
/// Also used internally for non-class requests.
#[derive(Eq, PartialEq, Debug)]
pub enum ControlInResult {
    /// Ignore the request and pass it to the next class.
    Ignore,

    /// Accept the request and return the number of bytes of data in the parameter.
    Ok(usize),

    /// Report an error to the host.
    Err,
}