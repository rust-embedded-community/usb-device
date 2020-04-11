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
/// All the optional methods are callbacks that may be called by
/// [`UsbCore::poll`](crate::usbcore::UsbCore::poll). You can leave any method you don't need
/// unimplemented.
///
/// The heart of this trait is the [`configure`](UsbClass::configure) method which implements a DSL
/// for USB descriptors.
pub trait UsbClass<U: UsbCore> {
    /// Called multiple times in the class lifecycle to access its resources such as endpoints and
    /// descriptor values. The class should call the methods in `config` with all of its resources.
    /// This is used both to modify their internals (such as assign hardware endpoints to endpoints
    /// at the allocation phase) and to get their values (such as the values of descriptors in the
    /// descriptor generation phase).
    ///
    /// Implementors must follow these rules when implementing this method:
    ///
    /// * All allocatable resources that the class wants to use (endpoints, interface handles and
    ///   string handles) must be passed in to `config`'s respective methods inside this method so
    ///   that they can be allocated.
    /// * Each resource must only be passed once, because resources cannot be allocated twice.
    /// * And most importantly, the method *must* pass in the same resources every time it is
    ///   called. If using any conditional resources or descriptors, the conditions must not change
    ///   while the class exists or the USB device will misbehave.
    ///
    /// Notably, descriptor values are returned to the host in the order they appear in this method.
    /// Many USB classes use the positions of class-specific descriptors to associate them with
    /// adjacent interface or endpoint descriptors.
    ///
    /// # Errors
    ///
    /// Any errors returned by `Config`. Implementors should propagate errors using `?`. Do not use
    /// `unwrap` because some `config` method calls return errors during normal operation.
    ///
    /// # Example
    ///
    /// This is a USB class that implements two interfaces. The first one has only one alternate
    /// setting, but the second interface has two alternate settings with different endpoints types.
    /// The second interface also has a description.
    ///
    /// ```
    /// use usb_device::class_prelude::*;
    /// use usb_device::Result;
    ///
    /// struct SampleClass<U: UsbCore> {
    ///     if_one: InterfaceHandle,
    ///     if_two: InterfaceHandle,
    ///     if_two_description: StringHandle,
    ///     ep1: EndpointOut<U>,
    ///     ep2_bulk: EndpointIn<U>,
    ///     ep2_interrupt: EndpointIn<U>,
    /// }
    ///
    /// impl<U: UsbCore> SampleClass<U> {
    ///     pub fn new() -> Self {
    ///         Self {
    ///             if_one: InterfaceHandle::new(),
    ///             if_two: InterfaceHandle::new(),
    ///             if_two_description: StringHandle::new(),
    ///             ep1: EndpointConfig::bulk(16).into(),
    ///             ep2_bulk: EndpointConfig::bulk(64).into(),
    ///             ep2_interrupt: EndpointConfig::interrupt(64, 10).into(),
    ///         }
    ///     }
    /// }
    ///
    /// impl<U: UsbCore> UsbClass<U> for SampleClass<U> {
    ///     fn configure(&mut self, mut config: Config<U>) -> Result<()> {
    ///         config.string(&mut self.if_two_description, "I am interface 2");
    ///
    ///         config
    ///             .interface(
    ///                 &mut self.if_one,
    ///                 InterfaceDescriptor::class(0xff))? // vendor specific
    ///             .descriptor(0x01, &[0x17, 0x37])? // class-specific descriptor
    ///             .endpoint_out(&mut self.ep1)?;
    ///
    ///         config
    ///             .interface(
    ///                 &mut self.if_two,
    ///                 InterfaceDescriptor::class(0xff).description(&self.if_two_description))?
    ///             // Endpoints/descriptors for alternate setting 0
    ///             .endpoint_in(&mut self.ep2_bulk)?
    ///             .next_alt_setting()?
    ///             // Endpoints/descriptors alt alternate settings 1
    ///             .endpoint_in(&mut self.ep2_interrupt)?;
    ///
    ///         Ok(())
    ///     }
    /// }
    /// ```
    ///
    /// # Implementation
    ///
    /// While class implementors don't really need to worry about when and why this method is called
    /// exactly, it could be interesting to know. Internally, `config` implements a visitor that
    /// allows various visitor implementations to examine and modify the resources and values passed
    /// to it. Visitors are used in the following phases:
    ///
    /// **Resource allocation**
    ///
    /// Each endpoint is allocated hardware resources using the platform-specific allocator. String
    /// and interface handles get assigned sequential values. Because at this phase not all
    /// resources contain valid values yet, converting them to their numerical representation (`u8`)
    /// will return dummy values. Therefore any custom descriptors ([`Config::descriptor`]) that
    /// reference them will actually be invalid, but it does not matter because in this phase the
    /// descriptor values are all discarded.
    ///
    /// **Configuration descriptor generation**
    ///
    /// At device enumeration time, the configuration descriptors from each class must be gathered.
    /// At this phase, the method is called again with a different `Config` that, instead of
    /// allocating resources, writes descriptors for them as well as any class-specific descriptors
    /// into a buffer which will then be sent to the host.
    ///
    /// **String descriptor access**
    ///
    /// When the USB host requests a string descriptor, the method will be called with a `Config`
    /// that only responds to `string`, and when the requested string descriptor is found, it writes
    /// its value into a buffer. This also causes `config` to return the hidden `UsbError::Break` to
    /// exit out of the configuration method early.
    ///
    /// **Enabling endpoints**
    ///
    /// When the USB host configures the device or switches interface alternate settings, the method
    /// will be called with a `Config` that searches for matching endpoints enables them. It also
    /// stores the active alternate setting number inside the interface handle. If switching to a
    /// new alternate settings the method is first called with a visitor that disables all endpoints
    /// of the interface to ensure two endpoints belonging to different alternate settings aren't
    /// briefly enabled at the same time.
    ///
    /// **"Get interface" requests**
    ///
    /// When the USB hosts requests the current alternate setting of an interface, another `Config`
    /// is used to get the current alternate setting from the matching interface handle. This also
    /// uses the hidden `UsbError::Break`.
    ///
    fn configure(&mut self, config: Config<U>) -> Result<()>;

    /// Called when a GET_DESCRIPTOR request is received for a BOS descriptor. When called, the
    /// implementation should write its blobs such as capability descriptors into `writer`. The BOS
    /// descriptor header will be written by [`UsbDevice`](crate::device::UsbDevice) and shouldn't
    /// be written by classes.
    ///
    /// # Errors
    ///
    /// Any errors returned by `BosWriter`. Implementors should propagate errors using `?`.
    fn get_bos_descriptors(&self, writer: &mut BosWriter) -> Result<()> {
        let _ = writer;
        Ok(())
    }

    /// Called after a USB reset.
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
    /// Use [`EndpointOutSet::contains`] to check which endpoints have received data.
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
    /// Use [`EndpointInSet::contains`] to check which endpoints have received data.
    fn endpoint_in_complete(&mut self, eps: EndpointInSet) {
        let _ = eps;
    }
}

/// Set of OUT endpoint addresses.
#[derive(Debug)]
pub struct EndpointOutSet(pub(crate) u16);

impl EndpointOutSet {
    /// Returns `true` if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the set contains the specified endpoint's address. Always returns `false`
    /// if the endpoint has not been allocated yet.
    pub fn contains<U: UsbCore>(&self, ep: &EndpointOut<U>) -> bool {
        ep.address_option().map(|a| (self.0 & (1 << a.number())) != 0).unwrap_or(false)
    }
}

/// Set of IN endpoint addresses.
#[derive(Debug)]
pub struct EndpointInSet(pub(crate) u16);

impl EndpointInSet {
    /// Return `true` if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the set contains the specified endpoint's address. Always returns `false`
    /// if the endpoint has not been allocated yet.
    pub fn contains<U: UsbCore>(&self, ep: &EndpointIn<U>) -> bool {
        ep.address_option().map(|a| (self.0 & (1 << a.number())) != 0).unwrap_or(false)
    }
}
