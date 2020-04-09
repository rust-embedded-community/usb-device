use crate::control;
use crate::control_pipe::ControlPipe;
use crate::usbcore::UsbCore;
use crate::{Result, UsbError};

/// Handle for a control IN transfer. When implementing a class, use the methods of this object to
/// response to the transfer with either data or an error (STALL condition). To ignore the request
/// and pass it on to the next class, simply don't call any method.
pub struct ControlIn<'p, 'r, U: UsbCore> {
    pipe: &'p mut ControlPipe<U>,
    req: &'r control::Request,
}

impl<'p, 'r, U: UsbCore> ControlIn<'p, 'r, U> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<U>, req: &'r control::Request) -> Self {
        ControlIn { pipe, req }
    }

    /// Gets the request from the SETUP packet.
    pub fn request(&self) -> &control::Request {
        self.req
    }

    /// Accepts the transfer with the supplied buffer.
    pub fn accept_with(self, data: &[u8]) -> Result<()> {
        self.pipe.accept_in(|buf| {
            if data.len() > buf.len() {
                return Err(UsbError::BufferOverflow);
            }

            buf[..data.len()].copy_from_slice(data);

            Ok(data.len())
        })
    }

    /// Accepts the transfer with the supplied static buffer.
    /// This method is useful when you have a large static descriptor to send as one packet.
    pub fn accept_with_static(self, data: &'static [u8]) -> Result<()> {
        self.pipe.accept_in_static(data)
    }

    /// Accepts the transfer with a callback that can write to the internal buffer of the control
    /// pipe. Can be used to avoid an extra copy.
    pub fn accept(self, f: impl FnOnce(&mut [u8]) -> Result<usize>) -> Result<()> {
        self.pipe.accept_in(f)
    }

    /// Rejects the transfer by stalling the pipe.
    pub fn reject(self) -> Result<()> {
        self.pipe.reject()
    }
}

/// Handle for a control OUT transfer. When implementing a class, use the methods of this object to
/// response to the transfer with an ACT or an error (STALL condition). To ignore the request and
/// pass it on to the next class, simply don't call any method.
pub struct ControlOut<'p, 'r, U: UsbCore> {
    pipe: &'p mut ControlPipe<U>,
    req: &'r control::Request,
}

impl<'p, 'r, U: UsbCore> ControlOut<'p, 'r, U> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<U>, req: &'r control::Request) -> Self {
        ControlOut { pipe, req }
    }

    /// Gets the request from the SETUP packet.
    pub fn request(&self) -> &control::Request {
        self.req
    }

    /// Gets the data from the data stage of the request. May be empty if there was no data stage.
    pub fn data(&self) -> &[u8] {
        self.pipe.data()
    }

    /// Accepts the transfer by succesfully responding to the status stage.
    pub fn accept(self) -> Result<()> {
        self.pipe.accept_out()
    }

    /// Rejects the transfer by stalling the pipe.
    pub fn reject(self) -> Result<()> {
        self.pipe.reject()
    }
}
