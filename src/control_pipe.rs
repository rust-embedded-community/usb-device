use core::cmp::min;
use core::convert::TryInto;
use crate::{Result, UsbDirection, UsbError};
use crate::bus::UsbBus;
use crate::control::Request;
use crate::endpoint::{Endpoint, EndpointIn, EndpointOut, OutPacketType};

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
//
// ControlPipe always reserves at least 8 bytes of space at the end of the buffer for a possible
// SETUP packet, so in some rare combinations of max_packet_size_0 and CONTROL_BUF_LEN, a transfer
// that would barely fit in the buffer can be rejected as too large.
#[cfg(not(feature = "control-buffer-256"))]
const CONTROL_BUF_LEN: usize = 128;
#[cfg(feature = "control-buffer-256")]
const CONTROL_BUF_LEN: usize = 256;

/// Buffers and parses USB control transfers.
pub struct ControlPipe<B: UsbBus> {
    ep_out: B::EndpointOut,
    ep_in: B::EndpointIn,
    state: ControlState,
    buf: [u8; CONTROL_BUF_LEN],
    static_in_buf: Option<&'static [u8]>,
    i: usize,
    len: usize,
}

impl<B: UsbBus> ControlPipe<B> {
    pub fn new<'a>(ep_out: B::EndpointOut, ep_in: B::EndpointIn) -> ControlPipe<B> {
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
        self.ep_out.enable();
        self.ep_in.enable();
        self.state = ControlState::Idle;
    }

    pub fn handle_out(&mut self) -> Option<Request> {
        // When reading a packet the buffer must always have at least 8 bytes of space for a SETUP
        // packet. If there is not enough space, make note of it and reset the buffer pointer to the
        // start to make space.
        let buffer_reset_for_setup = if self.buf.len() - self.i < 8 {
            self.i = 0;
            true
        } else {
            false
        };

        let (count, packet_type) = match self.ep_out.control_read_packet(&mut self.buf[self.i..]) {
            Ok(res) => res,
            Err(UsbError::WouldBlock) => {
                // This read should not block because this method is usually called when a packet is
                // known to be waiting to be read, but if the read somehow still blocked, and the
                // buffer was reset, this transfer has now failed.
                if buffer_reset_for_setup {
                    self.set_error();
                }

                return None;
            }
            Err(_) => {
                // Failed to read or buffer overflow (overflow is only possible if the host
                // sends more data than it indicated in the SETUP request)
                self.set_error();
                return None;
            },
        };

        if packet_type == OutPacketType::Setup {
            match (&self.buf[self.i..self.i+count]).try_into() {
                Ok(request) => self.handle_out_setup(request),
                Err(_) => {
                    // SETUP packet length is incorrect
                    self.set_error();
                    None
                },
            }
        } else if buffer_reset_for_setup {
            // Buffer was reset to reserve space for a potential SETUP, and this transfer has now
            // failed due to the buffer being too small.
            self.set_error();
            None
        } else {
            self.handle_out_data(count)
        }
    }

    fn handle_out_setup(&mut self, request: [u8; 8]) -> Option<Request> {
        let req = match Request::parse(&request) {
            Ok(req) => req,
            Err(_) => {
                // Failed to parse SETUP packet
                self.set_error();
                return None;
            },
        };

        // Now that we have properly parsed the setup packet, ensure the end-point is no longer in
        // a stalled state.
        self.ep_out.set_stalled(false);

        /*crate::println!("SETUP {:?} {:?} {:?} req:{} val:{} idx:{} len:{} {:?}",
            req.direction, req.request_type, req.recipient,
            req.request, req.value, req.index, req.length,
            self.state);*/

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

    fn handle_out_data(&mut self, count: usize) -> Option<Request> {
        match self.state {
            ControlState::DataOut(req) => {
                self.i += count;

                if self.i >= self.len {
                    self.state = ControlState::CompleteOut;
                    return Some(req);
                }
            },
            ControlState::StatusOut => {
                self.state = ControlState::Idle;
            },
            _ => {
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
                if self.ep_in.write_packet(&[]).is_err() {
                    // There isn't much we can do if the write fails, except to wait for another
                    // poll or for the host to resend the request.
                    return false;
                }

                self.state = ControlState::DataInLast;
            },
            ControlState::DataInLast => {
                self.ep_out.set_stalled(false);
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

        let buffer = self.static_in_buf.unwrap_or(&self.buf);
        if self.ep_in.write_packet(&buffer[self.i..(self.i+count)]).is_err() {
            // There isn't much we can do if the write fails, except to wait for another poll or for
            // the host to resend the request.
            return;
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

    pub fn accept_out(&mut self) -> Result<()> {
        match self.state {
            ControlState::CompleteOut => {},
            _ => return Err(UsbError::InvalidState),
        };

        self.ep_in.write_packet(&[]).ok();
        self.state = ControlState::StatusIn;
        Ok(())
    }

    pub fn accept_in(&mut self, f: impl FnOnce(&mut [u8]) -> Result<usize>) -> Result<()> {
        let req = match self.state {
            ControlState::CompleteIn(req) => req,
            _ => return Err(UsbError::InvalidState),
        };

        let len = f(&mut self.buf[..])?;

        if len > self.buf.len() {
            self.set_error();
            return Err(UsbError::BufferOverflow);
        }

        self.start_in_transfer(req, len)
    }

    pub fn accept_in_static(&mut self, data: &'static [u8]) -> Result<()> {
        let req = match self.state {
            ControlState::CompleteIn(req) => req,
            _ => return Err(UsbError::InvalidState),
        };

        self.static_in_buf = Some(data);

        self.start_in_transfer(req, data.len())
    }

    fn start_in_transfer(&mut self, req: Request, data_len: usize) -> Result<()> {
        self.len = min(data_len, req.length as usize);
        self.i = 0;
        self.state = ControlState::DataIn;
        self.write_in_chunk();

        Ok(())
    }

    pub fn reject(&mut self) -> Result<()> {
        if !self.waiting_for_response() {
            return Err(UsbError::InvalidState);
        }

        self.set_error();
        Ok(())
    }

    fn set_error(&mut self) {
        self.state = ControlState::Error;
        self.ep_out.set_stalled(true);
        self.ep_in.set_stalled(true);
    }
}
