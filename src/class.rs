use crate::control_pipe::ControlPipe;
use crate::Result;

pub use crate::allocator::{InterfaceHandle, StringHandle};
pub use crate::config::{Config, InterfaceConfig, InterfaceDescriptor};
pub use crate::control;
pub use crate::descriptor::BosWriter;
pub use crate::device::UsbDeviceState;
pub use crate::endpoint::{EndpointAddress, EndpointConfig, EndpointIn, EndpointOut};
pub use crate::usbcore::UsbCore;
pub use crate::UsbError;

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
    /// ```no_run
    /// use usb_device::class::*;
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
    fn get_bos_descriptors(&mut self, writer: &mut BosWriter) -> Result<()> {
        let _ = writer;
        Ok(())
    }

    /// Called after a USB reset.
    fn reset(&mut self) {}

    /// Called to inform the class that an interface alternate setting has been activated by the
    /// host.
    ///
    /// This method may be called for an interface you didn't allocate. Always use the `interface`
    /// and `alt_setting` values to check which interface has been activated.
    fn alt_setting_activated(&mut self, interface: InterfaceHandle, alt_setting: u8) {
        let _ = (interface, alt_setting);
    }

    /// Called whenever the `UsbDevice` is polled, unless the device is suspended or not connected
    /// to the USB bus.
    ///
    /// If the host suspends the bus after being connected or the device is disconnected, this
    /// method will be called once with [`PollEvent::device_state`] set to `Suspend` to notify the
    /// class.
    ///
    /// The `event` parameter contains the current state of the USB device, as well as methods for
    /// checking which endpoints have events (data or completed transfer notifications) available.
    fn poll(&mut self, event: &PollEvent) {
        let _ = event;
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
}

/// Event information for USB class polls.
pub struct PollEvent {
    pub(crate) device_state: UsbDeviceState,
    pub(crate) ep_out: u16,
    pub(crate) ep_in_complete: u16,
}

impl PollEvent {
    /// Gets the current USB device state.
    pub fn device_state(&self) -> UsbDeviceState {
        self.device_state
    }

    /// Convenience method for checking if the USB device is in the Configured state, which is the
    /// state most class operations will happen.
    pub fn is_configured(&self) -> bool {
        self.device_state == UsbDeviceState::Configured
    }

    /// Returns `true` if there are any events.
    pub fn has_events(&self) -> bool {
        (self.ep_out | self.ep_in_complete) != 0
    }

    /// Returns `true` if the endpoint has received data.
    ///
    /// Always returns `false` if the endpoint has not been allocated yet.
    ///
    /// If you also read from the endpoint without receiving this event, the data may already have
    /// been read., so be sure to handle `WouldBlock` even after checking.
    pub fn has_data<U: UsbCore>(&self, ep: &EndpointOut<U>) -> bool {
        ep.address_option()
            .map(|a| (self.ep_out & (1 << a.number())) != 0)
            .unwrap_or(false)
    }

    /// Returns `true` if the endpoint has just completed a transfer.
    ///
    /// Always returns `false` if the endpoint has not been allocated yet.
    ///
    /// Transmission completion is not indicated to be signaled once for every call to
    /// [`EndpointIn::write_packet`], however it will indicated some time after transmitting one or
    /// more packets has completed unless the device was reset and some packets were discarded. You
    /// can use [`EndpointIn::flush`] to check if all data sent so far has been transmitted.
    pub fn has_completed<U: UsbCore>(&self, ep: &EndpointIn<U>) -> bool {
        ep.address_option()
            .map(|a| (self.ep_in_complete & (1 << a.number())) != 0)
            .unwrap_or(false)
    }
}

/// Handle for a control IN transfer.
///
/// When implementing a class, use the methods of this object to
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

/// Handle for a control OUT transfer.
///
/// When implementing a class, use the methods of this object to
/// response to the transfer with an ACK or an error (STALL condition). To ignore the request and
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
