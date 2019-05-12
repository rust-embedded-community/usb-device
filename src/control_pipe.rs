use core::cmp::min;
use core::mem;
use crate::{Result, UsbDirection, UsbError};
use crate::bus::UsbBus;
use crate::control::Request;
use crate::endpoint::{EndpointIn, EndpointOut};

#[derive(PartialEq, Eq, Debug)]
#[allow(unused)]
enum ControlState {
    Idle,
    DataIn,
    DataInZlp,
    DataInLast,
    CompleteIn,
    StatusOut,
    CompleteOut,
    DataOut,
    StatusIn,
    Error,
}

// Maximum length of control transfer data stage in bytes. It might be necessary to make this
// non-const in the future.
const CONTROL_BUF_LEN: usize = 128;

/// Buffers and parses USB control transfers.
pub struct ControlPipe<'a, B: UsbBus> {
    ep_out: EndpointOut<'a, B>,
    ep_in: EndpointIn<'a, B>,
    state: ControlState,
    request: Option<Request>,
    buf: [u8; CONTROL_BUF_LEN],
    i: usize,
    len: usize,
}

impl<B: UsbBus> ControlPipe<'_, B> {
    pub fn new<'a>(ep_out: EndpointOut<'a, B>, ep_in: EndpointIn<'a, B>) -> ControlPipe<'a, B> {
        ControlPipe {
            ep_out,
            ep_in,
            state: ControlState::Idle,
            request: None,
            buf: unsafe { mem::uninitialized() },
            i: 0,
            len: 0,
        }
    }

    pub fn waiting_for_response(&self) -> bool {
        self.state == ControlState::CompleteOut || self.state == ControlState::CompleteIn
    }

    pub fn request(&self) -> &Request {
        self.request.as_ref().unwrap()
    }

    pub fn data(&self) -> &[u8] {
        &self.buf[0..self.len]
    }

    pub fn reset(&mut self) {
        self.state = ControlState::Idle;
    }

    pub fn handle_setup<'p>(&'p mut self) -> Option<UsbDirection> {
        let count = match self.ep_out.read(&mut self.buf[..]) {
            Ok(count) => count,
            Err(UsbError::WouldBlock) => return None,
            Err(_) => {
                self.set_error();
                return None;
            }
        };

        let req = match Request::parse(&self.buf[0..count]) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                self.set_error();
                return None;
            },
        };

        /*sprintln!("SETUP {:?} {:?} {:?} req:{} val:{} idx:{} len:{} {:?}",
            req.direction, req.request_type, req.recipient,
            req.request, req.value, req.index, req.length,
            self.state);*/

        self.request = Some(req);

        if req.direction == UsbDirection::Out {
            // OUT transfer

            if req.length > 0 {
                // Has data stage

                if req.length as usize > self.buf.len() {
                    // Data stage won't fit in buffer
                    self.set_error();
                    return None;
                }

                self.i = 0;
                self.len = req.length as usize;
                self.state = ControlState::DataOut;
            } else {
                // No data stage

                self.len = 0;
                self.state = ControlState::CompleteOut;
                return Some(UsbDirection::Out);
            }
        } else {
            // IN transfer

            self.state = ControlState::CompleteIn;
            return Some(UsbDirection::In);
        }

        return None;
    }

    pub fn handle_out<'p>(&'p mut self) -> Option<UsbDirection> {
        match self.state {
            ControlState::DataOut => {
                let i = self.i;
                let count = match self.ep_out.read(&mut self.buf[i..]) {
                    Ok(count) => count,
                    Err(UsbError::WouldBlock) => return None,
                    Err(_) => {
                        // Failed to read or buffer overflow (overflow is only possible if the host
                        // sends more data than it indicated in the SETUP request)
                        self.set_error();
                        return None;
                    },
                };

                self.i += count;

                if self.i >= self.len {
                    self.state = ControlState::CompleteOut;
                    return Some(UsbDirection::Out);
                }
            },
            ControlState::StatusOut => {
                self.ep_out.read(&mut []).ok();
                self.state = ControlState::Idle;
            },
            _ => {
                // Discard the packet
                self.ep_out.read(&mut []).ok();

                // Unexpected OUT packet
                self.set_error()
            },
        }

        return None;
    }

    pub fn handle_in_complete(&mut self) -> bool {
        match self.state {
            ControlState::DataIn => {
                self.write_in_chunk();
            },
            ControlState::DataInZlp => {
                match self.ep_in.write(&[]) {
                    Err(UsbError::WouldBlock) => return false,
                    Err(err) => panic!("{:?}", err),
                    _ => {},
                };

                self.state = ControlState::DataInLast;
            },
            ControlState::DataInLast => {
                self.ep_out.unstall();
                self.state = ControlState::StatusOut;
            },
            ControlState::StatusIn => {
                self.state = ControlState::Idle;
                return true;
            },
            _ => {
                // Unexpected IN packet
                self.set_error();
            }
        };

        return false;
    }

    fn write_in_chunk(&mut self) {
        let count = min(self.len - self.i, self.ep_in.max_packet_size() as usize);

        let count = match self.ep_in.write(&self.buf[self.i..(self.i+count)]) {
            Err(UsbError::WouldBlock) => return,
            Err(err) => panic!("{:?}", err),
            Ok(c) => c,
        };

        self.i += count;

        if self.i >= self.len {
            self.state = if count == self.ep_in.max_packet_size() as usize {
                ControlState::DataInZlp
            } else {
                ControlState::DataInLast
            };
        }
    }

    pub fn accept_out(&mut self) -> Result<()> {
        if self.state != ControlState::CompleteOut {
            return Err(UsbError::InvalidState);
        }

        self.ep_in.write(&[]).ok();
        self.state = ControlState::StatusIn;
        Ok(())
    }

    pub fn accept_in(&mut self, f: impl FnOnce(&mut [u8]) -> Result<usize>) -> Result<()> {
        if self.state != ControlState::CompleteIn {
            return Err(UsbError::InvalidState);
        }

        let len = f(&mut self.buf[..])?;

        if len > self.buf.len() {
            self.set_error();
            return Err(UsbError::BufferOverflow);
        }

        self.len = min(len, self.request.unwrap().length as usize);
        self.i = 0;
        self.state = ControlState::DataIn;
        self.write_in_chunk();

        Ok(())
    }

    pub fn reject(&mut self) -> Result<()> {
        if !(self.state == ControlState::CompleteOut || self.state == ControlState::CompleteIn) {
            return Err(UsbError::InvalidState);
        }

        self.set_error();
        Ok(())
    }

    fn set_error(&mut self) {
        self.state = ControlState::Error;
        self.ep_out.stall();
        self.ep_in.stall();
    }
}
