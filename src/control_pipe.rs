use crate::bus::UsbBus;
use crate::control::Request;
use crate::endpoint::{EndpointIn, EndpointOut};
use crate::{Result, UsbDirection, UsbError};
use core::cmp::min;

#[derive(Debug)]
#[allow(unused)]
enum ControlState {
    Idle,
    DataIn,
    DataInZlp,
    DataInLast,
    CompleteIn(Request),
    StatusOut,
    CompleteOut,
    DataOut(Request),
    StatusIn,
    Error,
}

// Maximum length of control transfer data stage in bytes. 128 bytes by default. You can define the
// feature "control-buffer-256" to make it 256 bytes if you have larger control transfers.
#[cfg(not(feature = "control-buffer-256"))]
const CONTROL_BUF_LEN: usize = 128;
#[cfg(feature = "control-buffer-256")]
const CONTROL_BUF_LEN: usize = 256;

/// Buffers and parses USB control transfers.
pub struct ControlPipe {
    ep_out: EndpointOut,
    ep_in: EndpointIn,
    state: ControlState,
    buf: [u8; CONTROL_BUF_LEN],
    static_in_buf: Option<&'static [u8]>,
    i: usize,
    len: usize,
}

impl ControlPipe {
    pub fn new<'a>(ep_out: EndpointOut, ep_in: EndpointIn) -> ControlPipe {
        ControlPipe {
            ep_out,
            ep_in,
            state: ControlState::Idle,
            buf: [0; CONTROL_BUF_LEN],
            static_in_buf: None,
            i: 0,
            len: 0,
        }
    }

    pub fn waiting_for_response(&self) -> bool {
        match self.state {
            ControlState::CompleteOut | ControlState::CompleteIn(_) => true,
            _ => false,
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.buf[0..self.len]
    }

    pub fn reset(&mut self) {
        self.state = ControlState::Idle;
    }

    pub fn handle_setup<B: UsbBus>(&mut self, bus: &mut B) -> Option<Request> {
        let count = match self.ep_out.read(bus, &mut self.buf[..]) {
            Ok(count) => count,
            Err(UsbError::WouldBlock) => return None,
            Err(_) => {
                self.set_error(bus);
                return None;
            }
        };

        let req = match Request::parse(&self.buf[0..count]) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                self.set_error(bus);
                return None;
            }
        };

        // Now that we have properly parsed the setup packet, ensure the end-point is no longer in
        // a stalled state.
        self.ep_out.unstall(bus);

        /*sprintln!("SETUP {:?} {:?} {:?} req:{} val:{} idx:{} len:{} {:?}",
        req.direction, req.request_type, req.recipient,
        req.request, req.value, req.index, req.length,
        self.state);*/

        if req.direction == UsbDirection::Out {
            // OUT transfer

            if req.length > 0 {
                // Has data stage

                if req.length as usize > self.buf.len() {
                    // Data stage won't fit in buffer
                    self.set_error(bus);
                    return None;
                }

                self.i = 0;
                self.len = req.length as usize;
                self.state = ControlState::DataOut(req);
            } else {
                // No data stage

                self.len = 0;
                self.state = ControlState::CompleteOut;
                return Some(req);
            }
        } else {
            // IN transfer

            self.state = ControlState::CompleteIn(req);
            return Some(req);
        }

        return None;
    }

    pub fn handle_out<B: UsbBus>(&mut self, bus: &mut B) -> Option<Request> {
        match self.state {
            ControlState::DataOut(req) => {
                let i = self.i;
                let count = match self.ep_out.read(bus, &mut self.buf[i..]) {
                    Ok(count) => count,
                    Err(UsbError::WouldBlock) => return None,
                    Err(_) => {
                        // Failed to read or buffer overflow (overflow is only possible if the host
                        // sends more data than it indicated in the SETUP request)
                        self.set_error(bus);
                        return None;
                    }
                };

                self.i += count;

                if self.i >= self.len {
                    self.state = ControlState::CompleteOut;
                    return Some(req);
                }
            }
            ControlState::StatusOut => {
                self.ep_out.read(bus, &mut []).ok();
                self.state = ControlState::Idle;
            }
            _ => {
                // Discard the packet
                self.ep_out.read(bus, &mut []).ok();

                // Unexpected OUT packet
                self.set_error(bus)
            }
        }

        return None;
    }

    pub fn handle_in_complete<B: UsbBus>(&mut self, bus: &mut B) -> bool {
        match self.state {
            ControlState::DataIn => {
                self.write_in_chunk(bus);
            }
            ControlState::DataInZlp => {
                if self.ep_in.write(bus, &[]).is_err() {
                    // There isn't much we can do if the write fails, except to wait for another
                    // poll or for the host to resend the request.
                    return false;
                }

                self.state = ControlState::DataInLast;
            }
            ControlState::DataInLast => {
                self.ep_out.unstall(bus);
                self.state = ControlState::StatusOut;
            }
            ControlState::StatusIn => {
                self.state = ControlState::Idle;
                return true;
            }
            _ => {
                // Unexpected IN packet
                self.set_error(bus);
            }
        };

        return false;
    }

    fn write_in_chunk<B: UsbBus>(&mut self, bus: &mut B) {
        let count = min(self.len - self.i, self.ep_in.max_packet_size() as usize);

        let buffer = self.static_in_buf.unwrap_or(&self.buf);
        let count = match self.ep_in.write(bus, &buffer[self.i..(self.i + count)]) {
            Ok(c) => c,
            // There isn't much we can do if the write fails, except to wait for another poll or for
            // the host to resend the request.
            Err(_) => return,
        };

        self.i += count;

        if self.i >= self.len {
            self.static_in_buf = None;

            self.state = if count == self.ep_in.max_packet_size() as usize {
                ControlState::DataInZlp
            } else {
                ControlState::DataInLast
            };
        }
    }

    pub fn accept_out<B: UsbBus>(&mut self, bus: &mut B) -> Result<()> {
        match self.state {
            ControlState::CompleteOut => {}
            _ => return Err(UsbError::InvalidState),
        };

        self.ep_in.write(bus, &[]).ok();
        self.state = ControlState::StatusIn;
        Ok(())
    }

    pub fn accept_in<B: UsbBus>(
        &mut self,
        bus: &mut B,
        f: impl FnOnce(&mut [u8]) -> Result<usize>,
    ) -> Result<()> {
        let req = match self.state {
            ControlState::CompleteIn(req) => req,
            _ => return Err(UsbError::InvalidState),
        };

        let len = f(&mut self.buf[..])?;

        if len > self.buf.len() {
            self.set_error(bus);
            return Err(UsbError::BufferOverflow);
        }

        self.start_in_transfer(bus, req, len)
    }

    pub fn accept_in_static<B: UsbBus>(&mut self, bus: &mut B, data: &'static [u8]) -> Result<()> {
        let req = match self.state {
            ControlState::CompleteIn(req) => req,
            _ => return Err(UsbError::InvalidState),
        };

        self.static_in_buf = Some(data);

        self.start_in_transfer(bus, req, data.len())
    }

    fn start_in_transfer<B: UsbBus>(
        &mut self,
        bus: &mut B,
        req: Request,
        data_len: usize,
    ) -> Result<()> {
        self.len = min(data_len, req.length as usize);
        self.i = 0;
        self.state = ControlState::DataIn;
        self.write_in_chunk(bus);

        Ok(())
    }

    pub fn reject<B: UsbBus>(&mut self, bus: &mut B) -> Result<()> {
        if !self.waiting_for_response() {
            return Err(UsbError::InvalidState);
        }

        self.set_error(bus);
        Ok(())
    }

    fn set_error<B: UsbBus>(&mut self, bus: &mut B) {
        self.state = ControlState::Error;
        self.ep_out.stall(bus);
        self.ep_in.stall(bus);
    }
}
