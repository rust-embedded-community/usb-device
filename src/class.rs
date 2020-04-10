use crate::allocator::InterfaceHandle;
use crate::config::Config;
use crate::descriptor::BosWriter;
use crate::device::UsbDeviceState;
use crate::endpoint::{EndpointOut, EndpointIn};
use crate::usbcore::UsbCore;
use crate::Result;

pub use crate::control_transfer::*;

/// A trait for implementing USB classes.
///
/// Most methods are optional callbacks that will be called by
/// [UsbCore::poll](crate::bus::UsbCore::poll)
pub trait UsbClass<U: UsbCore> {
    /// Handles all the things. TODO: Document this method!
    ///
    /// # Errors
    ///
    /// Any errors returned by `Config`. Implementors should propagate any error using `?`.
    fn configure(&mut self, config: Config<U>) -> Result<()>;

    /// Called when a GET_DESCRIPTOR request is received for a BOS descriptor. When called, the
    /// implementation should write its blobs such as capability descriptors into `writer`. The BOS
    /// descriptor itself will be written by [UsbDevice](crate::device::UsbDevice) and shouldn't be
    /// written by classes.
    fn get_bos_descriptors(&self, writer: &mut BosWriter) -> Result<()> {
        let _ = writer;
        Ok(())
    }

    /// Called after a USB reset after the bus reset sequence is complete.
    fn reset(&mut self) {}

    /// Called to inform the class that an interface alternate setting has been activated by the
    /// host.
    fn alt_setting_activated(&mut self, interface: InterfaceHandle, alt_setting: u8) {
        let _ = (interface, alt_setting);
    }

    /// Called whenever the `UsbDevice` is polled.
    fn poll(&mut self, state: UsbDeviceState) {
        let _ = state;
    }

    /// Called when a control request is received with direction HostToDevice.
    ///
    /// All requests are passed to all classes in turn, which can choose to accept, ignore or report
    /// an error. Classes can even choose to override standard requests, but doing that is rarely
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
    fn control_out(&mut self, xfer: ControlOut<U>) {
        let _ = xfer;
    }

    /// Called when a control request is received with direction DeviceToHost.
    ///
    /// All requests are passed to all classes in turn, which can choose to accept, ignore or report
    /// an error. Classes can even choose to override standard requests, but doing that is rarely
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
    fn control_in(&mut self, xfer: ControlIn<U>) {
        let _ = xfer;
    }

    /// Called when some endpoints may have received data (OUT packet).
    ///
    /// Use [`EndpointOutSet::container`] to check which endpoints have received data.
    fn endpoint_out(&mut self, eps: EndpointOutSet) {
        let _ = eps;
    }

    /// Called when some endpoints may have completed transmitting data (IN packet).
    ///
    /// This method is not guaranteed to be called once for every call to
    /// [`EndpointIn::write_packet`], but it is guaranteed to eventually be called when all packets
    /// have been transmitted. You can use [`EndpointIn::flush`] to check if all data has been
    /// transmitted.
    ///
    /// Use [`EndpointInSet::container`] to check which endpoints have received data.
    fn endpoint_in_complete(&mut self, eps: EndpointInSet) {
        let _ = eps;
    }
}

pub struct EndpointOutSet(pub(crate) u16);

impl EndpointOutSet {
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn contains<U: UsbCore>(&self, ep: &EndpointOut<U>) -> bool {
        ep.address_option().map(|a| (self.0 & (1 << a.number())) != 0).unwrap_or(false)
    }
}

pub struct EndpointInSet(pub(crate) u16);

impl EndpointInSet {
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn contains<U: UsbCore>(&self, ep: &EndpointIn<U>) -> bool {
        ep.address_option().map(|a| (self.0 & (1 << a.number())) != 0).unwrap_or(false)
    }
}
