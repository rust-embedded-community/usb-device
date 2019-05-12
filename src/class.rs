use crate::{Result, UsbError};
use crate::bus::{UsbBus, StringIndex};
use crate::descriptor::DescriptorWriter;
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::endpoint::EndpointAddress;

/// A trait implemented by USB class implementations.
pub trait UsbClass<B: UsbBus> {
    /// Called when a GET_DESCRIPTOR request is received for a configuration descriptor. When
    /// called, the implementation should write its interface, endpoint and any extra class
    /// descriptors into `writer`. The configuration descriptor itself will be written by
    /// [UsbDevice](crate::device::UsbDevice) and shouldn't be written by classes.
    ///
    /// # Errors
    ///
    /// Generally errors returned by `DescriptorWriter`. Implementors should propagate any errors
    /// using `?`.
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        let _ = writer;
        Ok (())
    }

    /// Gets a class-specific string descriptor.
    ///
    /// Note: All string descriptor requests are passed to all classes in turn, so implementations
    /// should return [`None`] if an unknown index is requested.
    ///
    /// # Arguments
    ///
    /// * `index` - A string index allocated earlier with
    ///   [`UsbAllocator`](crate::bus::UsbBusAllocator).
    /// * `lang_id` - The language ID for the string to retrieve.
    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        let _ = (index, lang_id);
        None
    }

    /// Called after a USB reset after the bus reset sequence is complete.
    fn reset(&mut self) { }

    /// Called whenever the `UsbDevice` is polled.
    fn poll(&mut self) { }

    /// Called when a control request is received with direction HostToDevice.
    ///
    /// All requests are passed to classes in turn, which can choose to accept, ignore or report an
    /// error. Classes can even choose to override standard requests, but doing that is rarely
    /// necessary.
    ///
    /// See [`ControlOut`] for how to respond to the transfer.
    ///
    /// When implementing your own class, you should ignore any requests that are not meant for your
    /// class so that any other classes in the composite device can process them.
    ///
    /// # Arguments
    ///
    /// * `req` - The request from the SETUP packet.
    /// * `xfer` - A handle to the transfer.
    fn control_out(&mut self, xfer: ControlOut<B>) {
        let _ = xfer;
    }

    /// Called when a control request is received with direction DeviceToHost.
    ///
    /// All requests are passed to classes in turn, which can choose to accept, ignore or report an
    /// error. Classes can even choose to override standard requests, but doing that is rarely
    /// necessary.
    ///
    /// See [`ControlIn`] for how to respond to the transfer.
    ///
    /// When implementing your own class, you should ignore any requests that are not meant for your
    /// class so that any other classes in the composite device can process them.
    ///
    /// # Arguments
    ///
    /// * `req` - The request from the SETUP packet.
    /// * `data` - Data to send in the DATA stage of the control transfer.
    fn control_in(&mut self, xfer: ControlIn<B>) {
        let _ = xfer;
    }

    /// Called when endpoint with address `addr` has received a SETUP packet. Implementing this
    /// shouldn't be necessary in most cases, but is provided for completeness' sake.
    ///
    /// Note: This method may be called for an endpoint address you didn't allocate, and in that
    /// case you should ignore the event.
    fn endpoint_setup(&mut self, addr: EndpointAddress) {
        let _ = addr;
    }

    /// Called when endpoint with address `addr` has received data (OUT packet).
    ///
    /// Note: This method may be called for an endpoint address you didn't allocate, and in that
    /// case you should ignore the event.
    fn endpoint_out(&mut self, addr: EndpointAddress) {
        let _ = addr;
    }

    /// Called when endpoint with address `addr` has completed transmitting data (IN packet).
    ///
    /// Note: This method may be called for an endpoint address you didn't allocate, and in that
    /// case you should ignore the event.
    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        let _ = addr;
    }
}

/// Handle for a control IN transfer. When implementing a class, use the methods of this object to
/// response to the transfer with either data or an error (STALL condition). To ignore the request
/// and pass it on to the next class, simply don't call any method.
pub struct ControlIn<'a, 'p, B: UsbBus>(&'p mut ControlPipe<'a, B>);

impl<'a, 'p, B: UsbBus> ControlIn<'a, 'p,  B> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<'a, B>) -> Self {
        ControlIn(pipe)
    }

    /// Gets the request from the SETUP packet.
    pub fn request(&self) -> &control::Request {
        self.0.request()
    }

    /// Accepts the transfer with the supplied buffer.
    pub fn accept_with(self, data: &[u8]) -> Result<()> {
        self.0.accept_in(|buf| {
            if data.len() > buf.len() {
                return Err(UsbError::BufferOverflow);
            }

            buf[..data.len()].copy_from_slice(data);

            Ok(data.len())
        })
    }

    /// Accepts the transfer with a callback that can write to the internal buffer of the control
    /// pipe. Can be used to avoid an extra copy.
    pub fn accept(self, f: impl FnOnce(&mut [u8]) -> Result<usize>) -> Result<()> {
        self.0.accept_in(f)
    }

    /// Rejects the transfer by stalling the pipe.
    pub fn reject(self) -> Result<()> {
        self.0.reject()
    }
}

/// Handle for a control OUT transfer. When implementing a class, use the methods of this object to
/// response to the transfer with an ACT or an error (STALL condition). To ignore the request and
/// pass it on to the next class, simply don't call any method.
pub struct ControlOut<'a, 'p, B: UsbBus>(&'p mut ControlPipe<'a, B>);

impl<'a, 'p, B: UsbBus> ControlOut<'a, 'p, B> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<'a, B>) -> Self {
        ControlOut(pipe)
    }

    /// Gets the request from the SETUP packet.
    pub fn request(&self) -> &control::Request {
        self.0.request()
    }

    /// Gets the data from the data stage of the request. May be empty if there was no data stage.
    pub fn data(&self) -> &[u8] {
        self.0.data()
    }

    /// Accepts the transfer by succesfully responding to the status stage.
    pub fn accept(self) -> Result<()> {
        self.0.accept_out()
    }

    /// Rejects the transfer by stalling the pipe.
    pub fn reject(self) -> Result<()> {
        self.0.reject()
    }
}