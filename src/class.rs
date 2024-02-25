use crate::bus::{InterfaceNumber, StringIndex, UsbBus};
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::descriptor::lang_id::LangID;
use crate::descriptor::{BosWriter, DescriptorWriter};
use crate::endpoint::EndpointAddress;
use crate::{Result, UsbError};

/// A trait for implementing USB classes.
///
/// All methods are optional callbacks that will be called by
/// [UsbBus::poll](crate::bus::UsbBus::poll)
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
        Ok(())
    }

    /// Called when a GET_DESCRIPTOR request is received for a BOS descriptor.
    /// When called, the implementation should write its blobs such as capability
    /// descriptors into `writer`. The BOS descriptor itself will be written by
    /// [UsbDevice](crate::device::UsbDevice) and shouldn't be written by classes.
    fn get_bos_descriptors(&self, writer: &mut BosWriter) -> Result<()> {
        let _ = writer;
        Ok(())
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
    /// * `lang_id` - The language ID for the string to retrieve. If the requested lang_id is not
    ///   valid it will default to EN_US.
    fn get_string(&self, index: StringIndex, lang_id: LangID) -> Option<&str> {
        let _ = (index, lang_id);
        None
    }

    /// Called after a USB reset after the bus reset sequence is complete.
    fn reset(&mut self) {}

    /// Called whenever the `UsbDevice` is polled.
    fn poll(&mut self) {}

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

    /// Called when the interfaces alternate setting state is requested.
    ///
    /// Note: This method may be called on interfaces, that are not relevant to this class.
    /// You should return `None, if `interface` belongs to an interface you don't know.
    fn get_alt_setting(&mut self, interface: InterfaceNumber) -> Option<u8> {
        let _ = interface;
        None
    }

    /// Called when the interfaces alternate setting state is altered.
    ///
    /// Note: This method may be called on interfaces, that are not relevant to this class.
    /// You should return `false`, if `interface` belongs to an interface you don't know.
    fn set_alt_setting(&mut self, interface: InterfaceNumber, alternative: u8) -> bool {
        let _ = (interface, alternative);
        false
    }
}

/// Handle for a control IN transfer. When implementing a class, use the methods of this object to
/// response to the transfer with either data or an error (STALL condition). To ignore the request
/// and pass it on to the next class, simply don't call any method.
pub struct ControlIn<'a, 'p, 'r, B: UsbBus> {
    pipe: &'p mut ControlPipe<'a, B>,
    req: &'r control::Request,
}

impl<'a, 'p, 'r, B: UsbBus> ControlIn<'a, 'p, 'r, B> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<'a, B>, req: &'r control::Request) -> Self {
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
pub struct ControlOut<'a, 'p, 'r, B: UsbBus> {
    pipe: &'p mut ControlPipe<'a, B>,
    req: &'r control::Request,
}

impl<'a, 'p, 'r, B: UsbBus> ControlOut<'a, 'p, 'r, B> {
    pub(crate) fn new(pipe: &'p mut ControlPipe<'a, B>, req: &'r control::Request) -> Self {
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
