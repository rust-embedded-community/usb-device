use core::cmp::min;
use heapless;
use ::{Result, UsbError};
use bus::{UsbBusWrapper, UsbBus, PollResult};
use endpoint::{EndpointType, EndpointIn, EndpointOut, EndpointAddress, EndpointDirection};
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

// Maximum length of control transfer data stage in bytes. It might be necessary to make this
// non-const in the future.
const CONTROL_BUF_LEN: usize = 128;

// Completely arbitrary value. Nobody needs more than 4, right?
const MAX_CLASSES: usize = 4;

// Maximum number of endpoints in one direction. Specified by the USB specification.
const MAX_ENDPOINTS: usize = 16;

/// Holds the current state and data buffer for control requests.
pub(crate) struct Control {
    state: ControlState,
    request: Option<control::Request>,
    pub(crate) buf: [u8; CONTROL_BUF_LEN],
    i: usize,
    len: usize,
    pub(crate) pending_address: u8,
}

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<'a, B: UsbBus + 'a> {
    pub(crate) bus: &'a B,
    control_out: EndpointOut<'a, B>,
    control_in: EndpointIn<'a, B>,

    pub(crate) info: UsbDeviceInfo<'a>,

    pub(crate) classes: heapless::Vec<&'a dyn UsbClass, [&'a dyn UsbClass; MAX_CLASSES]>,

    pub(crate) control: Control,
    pub(crate) device_state: UsbDeviceState,
    pub(crate) remote_wakeup_enabled: bool,
    pub(crate) self_powered: bool,
}

impl<'a, B: UsbBus + 'a> UsbDevice<'a, B> {
    /// Creates a [`UsbDeviceBuilder`] for constructing a new instance.
    pub fn new(bus: &'a UsbBusWrapper<B>, vid_pid: UsbVidPid) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder::new(bus, vid_pid)
    }

    pub(crate) fn build(
        bus: &'a UsbBusWrapper<B>,
        classes: &[&'a dyn UsbClass],
        info: UsbDeviceInfo<'a>)
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

            classes: heapless::Vec::new(),

            control: Control {
                state: ControlState::Idle,
                request: None,
                buf: [0; 128],
                i: 0,
                len: 0,
                pending_address: 0,
            },

            device_state: UsbDeviceState::Default,
            remote_wakeup_enabled: false,
            self_powered: false,
        };

        dev.classes.extend_from_slice(classes).unwrap();

        dev.reset();

        dev
    }

    /// Gets the current state of the device.
    ///
    /// In general class traffic is only possible in the `Configured` state.
    pub fn state(&self) -> UsbDeviceState {
        self.device_state
    }

    /// Gets whether host remote wakeup has been enabled by the host.
    pub fn remote_wakeup_enabled(&self) -> bool {
        self.remote_wakeup_enabled
    }

    /// Gets whether the device is currently self powered.
    pub fn self_powered(&self) -> bool {
        self.self_powered
    }

    /// Sets whether the device is currently self powered.
    pub fn set_self_powered(&mut self, is_self_powered: bool) {
        self.self_powered = is_self_powered;
    }

    /// Forces a reset on the UsbBus.
    pub fn force_reset(&mut self) -> Result<()> {
        self.bus.force_reset()
    }

    /// Polls the [`UsbBus`] for new events and dispatches them accordingly. This should be called
    /// periodically more often than once every 10 milliseconds to stay USB compliant, or from an
    /// interrupt handler.
    pub fn poll(&mut self) {
        let pr = self.bus.poll();

        if self.device_state == UsbDeviceState::Suspend {
            if !(pr == PollResult::Suspend || pr == PollResult::None) {
                self.bus.resume();
                self.device_state = UsbDeviceState::Default;
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
                    for i in 1..MAX_ENDPOINTS {
                        if (ep_setup & bit) != 0 {
                            for cls in &self.classes {
                                cls.endpoint_setup(
                                    EndpointAddress::from_parts(i, EndpointDirection::Out));
                            }
                        } else if (ep_out & bit) != 0 {
                            for cls in &self.classes {
                                cls.endpoint_out(
                                    EndpointAddress::from_parts(i, EndpointDirection::Out));
                            }
                        }

                        if (ep_in_complete & bit) != 0 {
                            for cls in &self.classes {
                                cls.endpoint_in_complete(
                                    EndpointAddress::from_parts(i, EndpointDirection::In));
                            }
                        }

                        bit <<= 1;
                    }
                }
            },
            PollResult::Resume => { }
            PollResult::Suspend => {
                self.bus.suspend();
                self.device_state = UsbDeviceState::Suspend;
            }
        }
    }

    fn reset(&mut self) {
        self.bus.reset();

        self.device_state = UsbDeviceState::Default;
        self.remote_wakeup_enabled = false;

        self.control.state = ControlState::Idle;
        self.control.pending_address = 0;

        for cls in &self.classes {
            cls.reset().unwrap();
        }
    }

    fn handle_control_setup(&mut self) {
        let count = self.control_out.read(&mut self.control.buf[..]).unwrap();

        let req = match control::Request::parse(&self.control.buf[0..count]) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                return self.set_control_error()
            },
        };

        /*sprintln!("SETUP {:?} {:?} {:?} req:{} val:{} idx:{} len:{} {:?}",
            req.direction, req.request_type, req.recipient,
            req.request, req.value, req.index, req.length,
            control.state);*/

        self.control.request = Some(req);

        if req.direction == control::Direction::HostToDevice {
            if req.length > 0 {
                if req.length as usize > self.control.buf.len() {
                    // Transfer length won't fit in buffer
                    return self.set_control_error();
                }

                self.control.i = 0;
                self.control.len = req.length as usize;
                self.control.state = ControlState::DataOut;
            } else {
                self.control.len = 0;
                self.complete_control_out();
            }
        } else {
            let mut res = ControlInResult::Ignore;

            for cls in &self.classes {
                res = cls.control_in(&req, &mut self.control.buf);

                if res != ControlInResult::Ignore {
                    break;
                }
            }

            if res == ControlInResult::Ignore && req.request_type == control::RequestType::Standard {
                res = self.standard_control_in(&req);
            }

            if let ControlInResult::Ok(count) = res {
                self.control.i = 0;
                self.control.len = min(count, req.length as usize);
                self.control.state = ControlState::DataIn;

                self.write_control_in_chunk();
            } else {
                // Nothing accepted the request or there was an error
                self.set_control_error();
            }
        }
    }

    fn handle_control_out(&mut self) {
        match self.control.state {
            ControlState::DataOut => {
                let i = self.control.i;
                let count = match self.control_out.read(&mut self.control.buf[i..]) {
                    Ok(count) => count,
                    Err(_) => {
                        // Failed to read or buffer overflow (overflow is only possible if the host
                        // sends more data than indicated in the SETUP request)
                        return self.set_control_error()
                    },
                };

                self.control.i += count;

                if self.control.i >= self.control.len {
                    self.complete_control_out();
                }
            },
            ControlState::StatusOut => {
                self.control_out.read(&mut []).unwrap();
                self.control.state = ControlState::Idle;
            },
            _ => {
                // Discard the packet
                self.control_out.read(&mut self.control.buf[..]).ok();

                // Unexpected OUT packet
                self.set_control_error()
            },
        }
    }

    fn handle_control_in_complete(&mut self) {
        match self.control.state {
            ControlState::DataIn => {
                self.write_control_in_chunk();
            },
            ControlState::DataInZlp => {
                match self.control_in.write(&[]) {
                    Err(UsbError::Busy) => return,
                    Err(err) => panic!("{:?}", err),
                    _ => {},
                };

                self.control.state = ControlState::DataInLast;
            },
            ControlState::DataInLast => {
                self.control_out.unstall();
                self.control.state = ControlState::StatusOut;
            }
            ControlState::StatusIn => {
                if self.control.pending_address != 0 {
                    // SET_ADDRESS is really handled after the status packet has been sent
                    self.bus.set_device_address(self.control.pending_address);
                    self.device_state = UsbDeviceState::Addressed;
                    self.control.pending_address = 0;
                }

                self.control.state = ControlState::Idle;
            },
            _ => {
                // Unexpected IN packet
                self.set_control_error();
            }
        };
    }

    fn write_control_in_chunk(&mut self) {
        let count = min(self.control.len - self.control.i, self.info.max_packet_size_0 as usize);

        let count = match self.control_in.write(&self.control.buf[self.control.i..(self.control.i+count)]) {
            Err(UsbError::Busy) => return,
            Err(err) => panic!("{:?}", err),
            Ok(c) => c,
        };

        self.control.i += count;

        if self.control.i >= self.control.len {
            self.control.state = if count == self.info.max_packet_size_0 as usize {
                ControlState::DataInZlp
            } else {
                ControlState::DataInLast
            };
        }
    }

    fn complete_control_out(&mut self) {
        let req = self.control.request.take().unwrap();

        let mut res = ControlOutResult::Ignore;

        {
            let buf = &self.control.buf[..self.control.len];

            for cls in &self.classes {
                res = cls.control_out(&req, buf);

                if res != ControlOutResult::Ignore {
                    break;
                }
            }
        }

        if res == ControlOutResult::Ignore && req.request_type == control::RequestType::Standard {
            res = self.standard_control_out(&req);
        }

        if res == ControlOutResult::Ok {
            // Send empty packet to indicate success
            self.control_in.write(&[]).ok();
            self.control.state = ControlState::StatusIn;
        } else {
            // Nothing accepted the request or there was an error
            self.set_control_error();
        }
    }

    fn set_control_error(&mut self) {
        self.control.state = ControlState::Error;
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