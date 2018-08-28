use core::cmp::min;
use core::mem;
use core::cell::{Cell, RefCell};
use bus::{UsbBus, EndpointType, EndpointPair, EndpointIn, EndpointOut};
use control;
use class::UsbClass;
use device_info::UsbDeviceInfo;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum DeviceState {
    Default,
    Addressed,
    Configured,
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

pub struct UsbDevice<'a, T: UsbBus + 'a> {
    bus: &'a T,
    control_in: EndpointIn<'a, T>,
    control_out: EndpointOut<'a, T>,

    pub(crate) info: UsbDeviceInfo<'a>,

    class_arr: [&'a dyn UsbClass; 8],
    class_count: usize,

    control: RefCell<Control>,
    pub(crate) device_state: Cell<DeviceState>,
    pub(crate) pending_address: Cell<u8>,
}

impl<'a, T: UsbBus + 'a> UsbDevice<'a, T> {
    pub fn new(bus: &'a T, info: UsbDeviceInfo<'a>, classes: &[&'a dyn UsbClass]) -> UsbDevice<'a, T> {
        let (control_out, control_in) = EndpointPair::new(bus, 0)
            .split(EndpointType::Control, info.max_packet_size_0 as u16);

        let mut dev = UsbDevice {
            bus,
            control_in,
            control_out,

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
            device_state: Cell::new(DeviceState::Default),
            pending_address: Cell::new(0),
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

    pub fn state(&self) -> DeviceState {
        self.device_state.get()
    }

    fn reset(&self) {
        self.bus.reset();

        self.device_state.set(DeviceState::Default);

        let mut control = self.control.borrow_mut();
        control.state = ControlState::Idle;

        self.pending_address.set(0);

        self.control_out.configure().unwrap();
        self.control_in.configure().unwrap();

        for cls in self.classes() {
            cls.reset().unwrap();
        }
    }

    pub fn poll(&self) {
        let pr = self.bus.poll();

        if pr.reset {
            self.reset();
            return;
        }

        if pr.setup {
            self.handle_control_setup();
        } else if pr.ep_out & 1 != 0 {
            self.handle_control_out();
        }

        if pr.ep_in_complete & 1 != 0 {
            self.handle_control_in_complete();
        }

        for i in 1..(MAX_ENDPOINTS as u8) {
            if pr.ep_out & (1 << i) != 0 {
                for cls in self.classes() {
                    cls.endpoint_out(i);
                }
            }

            if pr.ep_in_complete & (1 << i) != 0 {
                for cls in self.classes() {
                    cls.endpoint_out(i | 0x80);
                }
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
                self.control_in.write(&[]).unwrap();
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
                    self.device_state.set(DeviceState::Addressed);
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

        self.control_in.write(&control.buf[control.i..(control.i+count)]).unwrap();

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

#[derive(Eq, PartialEq, Debug)]
pub enum ControlOutResult {
    Ignore,
    Ok,
    Err,
}

#[derive(Eq, PartialEq, Debug)]
pub enum ControlInResult {
    Ignore,
    Ok(usize),
    Err,
}