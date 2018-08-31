use core::cmp::min;
use core::mem;
use core::cell::{Cell, RefCell};
use ::UsbError;
use bus::{UsbBus, PollResult};
use endpoint::{EndpointType, EndpointIn, EndpointOut};
use control;
use class::UsbClass;
pub use device_builder::{UsbDeviceBuilder, UsbVidPid};

/// The global state of the USB device.
///
/// In general class traffic is only possible in the `Configured` state.
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

struct Control {
    state: ControlState,
    request: Option<control::Request>,
    buf: [u8; 128],
    i: usize,
    len: usize,
}

const MAX_ENDPOINTS: usize = 16;

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<'a, B: UsbBus + 'a> {
    pub(crate) bus: &'a B,
    control_out: EndpointOut<'a, B>,
    control_in: EndpointIn<'a, B>,

    pub(crate) info: UsbDeviceInfo<'a>,

    class_arr: [&'a dyn UsbClass; 8],
    class_count: usize,

    control: RefCell<Control>,
    pub(crate) device_state: Cell<UsbDeviceState>,
    pub(crate) pending_address: Cell<u8>,
    pub(crate) remote_wakeup_enabled: Cell<bool>,
    pub(crate) self_powered: Cell<bool>,
    pub(crate) halted_eps: Cell<u32>,
}

impl<'a, B: UsbBus + 'a> UsbDevice<'a, B> {
    /// Creates a [`UsbDeviceBuilder`] for constructing a new instance.
    pub fn new(bus: &'a B, vid_pid: UsbVidPid) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder::new(bus, vid_pid)
    }

    pub(crate) fn build(bus: &'a B, classes: &[&'a dyn UsbClass], info: UsbDeviceInfo<'a>)
        -> UsbDevice<'a, B>
    {
        let alloc = bus.allocator();

        let mut dev = UsbDevice {
            bus,
            control_out: alloc.alloc(Some(0), EndpointType::Control,
                info.max_packet_size_0 as u16, 0).unwrap(),
            control_in: alloc.alloc(Some(0), EndpointType::Control,
                info.max_packet_size_0 as u16, 0).unwrap(),

            info,

            class_arr: [unsafe { mem::uninitialized() }; 8],
            class_count: classes.len(),

            control: RefCell::new(Control {
                state: ControlState::Idle,
                request: None,
                buf: [0; 128],
                i: 0,
                len: 0,
            }),
            device_state: Cell::new(UsbDeviceState::Default),
            pending_address: Cell::new(0),
            remote_wakeup_enabled: Cell::new(false),
            self_powered: Cell::new(false),
            halted_eps: Cell::new(0),
        };

        assert!(classes.len() <= dev.class_arr.len());

        dev.class_arr[..dev.class_count].copy_from_slice(classes);

        dev.bus.enable();
        dev.reset();

        dev
    }

    pub(crate) fn classes(&self) -> &[&'a dyn UsbClass] {
        &self.class_arr[..self.class_count]
    }

    /// Gets the current state of the device.
    ///
    /// In general class traffic is only possible in the `Configured` state.
    pub fn state(&self) -> UsbDeviceState {
        self.device_state.get()
    }

    /// Gets whether host remote wakeup has been enabled by the host.
    pub fn remote_wakeup_enabled(self) -> bool {
        self.remote_wakeup_enabled.get()
    }

    /// Gets whether the device is currently self powered.
    pub fn self_powered(self) -> bool {
        self.self_powered.get()
    }

    /// Sets whether the device is currently self powered.
    pub fn set_self_powered(self, is_self_powered: bool) {
        self.self_powered.set(is_self_powered);
    }

    fn reset(&self) {
        self.bus.reset();

        self.device_state.set(UsbDeviceState::Default);
        self.remote_wakeup_enabled.set(false);
        self.halted_eps.set(0);

        let mut control = self.control.borrow_mut();
        control.state = ControlState::Idle;

        self.pending_address.set(0);

        for cls in self.classes() {
            cls.reset().unwrap();
        }
    }

    /// Polls the [`UsbBus`] for new events and dispatches them accordingly. This should be called
    /// periodically  more often than once every 10 milliseconds to stay USB-compliant, or
    /// from an interrupt handler.
    pub fn poll(&self) {
        let pr = self.bus.poll();

        if self.device_state.get() == UsbDeviceState::Suspend {
            if !(pr == PollResult::Suspend || pr == PollResult::None) {
                self.bus.resume();
                self.device_state.set(UsbDeviceState::Default)
            } else {
                return;
            }
        }

        match pr {
            PollResult::None => { }
            PollResult::Reset => self.reset(),
            PollResult::Data { ep_out, ep_in_complete, ep_setup } => {
                // Combine bit fields for quick tests
                let all = ep_out | ep_in_complete | ep_setup;

                // Pending events for endpoint 0?
                if (all & 1) != 0 {
                    if (ep_setup & 1) != 0 {
                        self.handle_control_setup();
                    } else if (ep_out & 1) != 0 {
                        self.handle_control_out();
                    }

                    if (ep_in_complete & 1) != 0 {
                        self.handle_control_in_complete();
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
                self.device_state.set(UsbDeviceState::Suspend);
            }
        }
    }

    fn handle_control_setup(&self) {
        let mut control = self.control.borrow_mut();

        let count = self.control_out.read(&mut control.buf[..]).unwrap();

        let req = match control::Request::parse(&control.buf[0..count]) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                return self.set_control_error(&mut control)
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
                    return self.set_control_error(&mut control);
                }

                control.i = 0;
                control.len = req.length as usize;
                control.state = ControlState::DataOut;
            } else {
                control.len = 0;
                self.complete_control_out(&mut control);
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

                self.write_control_in_chunk(&mut control);
            } else {
                // Nothing accepted the request or there was an error
                self.set_control_error(&mut control);
            }
        }
    }

    fn handle_control_out(&self) {
        let mut control = self.control.borrow_mut();

        match control.state {
            ControlState::DataOut => {
                let i = control.i;
                let count = match self.control_out.read(&mut control.buf[i..]) {
                    Ok(count) => count,
                    Err(_) => {
                        // Failed to read or buffer overflow (overflow is only possible if the host
                        // sends more data than indicated in the SETUP request)
                        return self.set_control_error(&mut control)
                    },
                };

                control.i += count;

                if control.i >= control.len {
                    self.complete_control_out(&mut control);
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
                self.set_control_error(&mut control)
            },
        }
    }

    fn handle_control_in_complete(&self) {
        let mut control = self.control.borrow_mut();

        match control.state {
            ControlState::DataIn => {
                self.write_control_in_chunk(&mut control);
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
                let addr = self.pending_address.replace(0);
                if addr != 0 {
                    // SET_ADDRESS is really handled after the status packet has been sent
                    self.bus.set_device_address(addr);
                    self.device_state.set(UsbDeviceState::Addressed);
                }

                control.state = ControlState::Idle;
            },
            _ => {
                // Unexpected IN packet
                self.set_control_error(&mut control);
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
                res = self.standard_control_out(&req, buf);
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
    /// Ignored the request and pass it to the next class.
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