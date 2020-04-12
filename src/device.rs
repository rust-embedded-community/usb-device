use crate::allocator::{self, InterfaceHandle, StringHandle, UsbAllocator};
use crate::class::{ControlIn, ControlOut, UsbClass, PollEvent};
use crate::config::{Config, ConfigVisitor, InterfaceDescriptor};
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::descriptor::{
    descriptor_type, lang_id, BosWriter, ConfigurationDescriptorWriter, DescriptorWriter,
};
pub use crate::device_builder::{UsbDeviceBuilder, UsbVidPid};
use crate::endpoint::{EndpointConfig, EndpointCore, EndpointIn, EndpointOut};
use crate::usbcore::{PollResult, UsbCore, UsbEndpoint};
use crate::{Result, UsbDirection, UsbError};

/// The global state of the USB device.
///
/// In general class traffic is only possible in the `Configured` state.
#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum UsbDeviceState {
    /// The USB device has just been created or reset.
    Default,

    /// The USB device has received an address from the host.
    Addressed,

    /// The USB device has been configured and is fully functional.
    Configured,

    /// The USB device has been suspended by the host or it has been unplugged from the USB bus.
    Suspend,
}

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<U: UsbCore> {
    usb: U,
    config: DeviceConfig,
    control: ControlPipe<U>,
    device_state: UsbDeviceState,
    remote_wakeup_enabled: bool,
    self_powered: bool,
    pending_address: u8,
}

pub(crate) struct DeviceConfig {
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer: Option<&'static str>,
    pub product: Option<&'static str>,
    pub serial_number: Option<&'static str>,
    pub self_powered: bool,
    pub supports_remote_wakeup: bool,
    pub composite_with_iads: bool,
    pub max_power: u8,
}

/// The bConfiguration value for the not configured state.
pub const CONFIGURATION_NONE: u8 = 0;

/// The bConfiguration value for the single configuration supported by this device.
pub const CONFIGURATION_VALUE: u8 = 1;

pub(crate) type ClassList<'a, U> = [&'a mut dyn UsbClass<U>];

impl<U: UsbCore> UsbDevice<U> {
    pub(crate) fn build(
        mut usb: U,
        config: DeviceConfig,
        classes: &mut ClassList<U>,
    ) -> Result<UsbDevice<U>> {
        let mut ep_alloc = usb.create_allocator();

        let control = ControlPipe::new(&mut ep_alloc, config.max_packet_size_0)?;

        Config::visit(classes, &mut UsbAllocator::new(&mut ep_alloc))?;

        usb.enable(ep_alloc)?;

        Ok(UsbDevice {
            usb: usb,
            config,
            control,
            device_state: UsbDeviceState::Default,
            remote_wakeup_enabled: false,
            self_powered: false,
            pending_address: 0,
        })
    }

    /// Gets a reference to the [`UsbCore`] implementation used by this `UsbDevice`. You can use this
    /// to call platform-specific methods on the `UsbCore`.
    ///
    /// While it is also possible to call the standard `UsbCore` trait methods through this
    /// reference, it is not recommended as it can cause the device to misbehave.
    pub fn usb(&self) -> &U {
        &self.usb
    }

    /// Gets a mutabe reference to the [`UsbCore`] implementation used by this `UsbDevice`. You can
    /// use this to call platform-specific methods on the `UsbCore`.
    ///
    /// While it is also possible to call the standard `UsbCore` trait methods through this
    /// reference, it is not recommended as it can cause the device to misbehave.
    pub fn usb_mut(&mut self) -> &U {
        &mut self.usb
    }

    /// Gets the current state of the device.
    ///
    /// In general class traffic is only possible in the `Configured` state.
    pub fn state(&self) -> UsbDeviceState {
        self.device_state
    }

    /// Gets whether host remote wakeup has been enabled by the host.
    pub fn remote_wakeup_enabled(&self) -> bool {
        self.remote_wakeup_enabled
    }

    /// Gets whether the device is currently self powered.
    pub fn self_powered(&self) -> bool {
        self.self_powered
    }

    /// Sets whether the device is currently self powered.
    pub fn set_self_powered(&mut self, is_self_powered: bool) {
        self.self_powered = is_self_powered;
    }

    /// Polls the [`UsbCore`] for new events and dispatches them to the provided classes. Returns
    /// `Ok` if one of the classes may have data available for reading or be ready for writing,
    /// `WouldBlock` if there is no new data available, or another error if an error occurred.. This
    /// should be called periodically as often as possible for the best data rate, or preferably
    /// from an interrupt handler. Must be called at least once every 10 milliseconds while
    /// connected to the USB host to be compliant with the USB specification.
    ///
    /// Note: The list of classes passed in must be the same classes in the same order as the ones
    /// used when creating the device or the device will misbehave.
    pub fn poll(&mut self, classes: &mut ClassList<'_, U>) -> Result<()> {
        let pr = self.usb.poll();

        if self.device_state == UsbDeviceState::Suspend {
            match pr {
                Ok(PollResult::Suspend) | Err(UsbError::WouldBlock) => {
                    return Err(UsbError::WouldBlock);
                }

                _ => {
                    self.usb.resume()?;
                }
            }
        }

        let mut ev_ep_out: u16 = 0;
        let mut ev_ep_in_complete: u16 = 0;

        if let Ok(pr) = &pr {
            match pr {
                PollResult::Resume => { /* handled above */ }
                PollResult::Reset => {
                    self.reset(classes)?;
                }
                PollResult::Data {
                    mut ep_out,
                    mut ep_in_complete,
                } => {
                    // Handle EP0-IN conditions first. When both EP0-IN and EP0-OUT have completed,
                    // it is possible that EP0-OUT is a zero-sized out packet to complete the STATUS
                    // phase of the control transfer. We have to process EP0-IN first to update our
                    // internal state properly.
                    if (ep_in_complete & 1) != 0 {
                        let completed = self.control.handle_in_complete()?;

                        if !U::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                            if completed && self.pending_address != 0 {
                                self.usb.set_device_address(self.pending_address)?;
                                self.pending_address = 0;

                                self.device_state = UsbDeviceState::Addressed;
                            }
                        }

                        ep_in_complete &= !1;
                    }

                    // Handle EP0-OUT second.
                    if (ep_out & 1) != 0 {
                        let req = self.control.handle_out()?;

                        match req {
                            Some(req) if req.direction == UsbDirection::In => {
                                self.control_in(classes, req)?;
                            }
                            Some(req) if req.direction == UsbDirection::Out => {
                                self.control_out(classes, req)?;
                            }
                            _ => (),
                        };

                        ep_out &= !1;
                    }

                    ev_ep_out = ep_out;
                    ev_ep_in_complete = ep_in_complete;
                }
                PollResult::Suspend => {
                    self.usb.suspend()?;
                    self.device_state = UsbDeviceState::Suspend;
                }
            };
        }

        let ev = PollEvent {
            device_state: self.device_state,
            ep_out: ev_ep_out,
            ep_in_complete: ev_ep_in_complete,
        };

        for cls in classes.iter_mut() {
            cls.poll(&ev);
        }

        pr.map(|_| ())
    }

    fn control_in(&mut self, classes: &mut ClassList<'_, U>, req: control::Request) -> Result<()> {
        use crate::control::{Recipient, Request};

        for cls in classes.iter_mut() {
            cls.control_in(ControlIn::new(&mut self.control, &req));

            if !self.control.waiting_for_response() {
                return Ok(());
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlIn::new(&mut self.control, &req);

            match (req.recipient, req.request) {
                (Recipient::Device, Request::GET_STATUS) => {
                    let status: u16 = 0x0000
                        | if self.self_powered { 0x0001 } else { 0x0000 }
                        | if self.remote_wakeup_enabled {
                            0x0002
                        } else {
                            0x0000
                        };

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Interface, Request::GET_STATUS) => {
                    let status: u16 = 0x0000;

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Endpoint, Request::GET_STATUS) => {
                    let ep_addr = ((req.index as u8) & 0x8f).into();

                    let status: u16 = 0x0000
                        | if self.usb.is_stalled(ep_addr)? {
                            0x0001
                        } else {
                            0x0000
                        };

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Device, Request::GET_DESCRIPTOR) => {
                    UsbDevice::get_descriptor(&self.config, classes, xfer)?;
                }

                (Recipient::Device, Request::GET_CONFIGURATION) => {
                    let config = match self.device_state {
                        UsbDeviceState::Configured => CONFIGURATION_VALUE,
                        _ => CONFIGURATION_NONE,
                    };

                    xfer.accept_with(&config.to_le_bytes())?;
                }

                (Recipient::Interface, Request::GET_INTERFACE) => {
                    let mut visitor = GetInterfaceVisitor::new(req.index as u8);

                    let res = Config::visit(classes, &mut visitor);

                    if let Some(alt_setting) = visitor.result() {
                        xfer.accept_with(&[alt_setting])?;
                    } else {
                        xfer.reject()?;
                    }

                    match res {
                        Ok(_) | Err(UsbError::Break) => {}
                        Err(err) => return Err(err),
                    }
                }

                _ => (),
            };
        }

        if self.control.waiting_for_response() {
            self.control.reject()?;
        }

        Ok(())
    }

    fn control_out(&mut self, classes: &mut ClassList<'_, U>, req: control::Request) -> Result<()> {
        use crate::control::{Recipient, Request};

        for cls in classes.iter_mut() {
            cls.control_out(ControlOut::new(&mut self.control, &req));

            if !self.control.waiting_for_response() {
                return Ok(());
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlOut::new(&mut self.control, &req);

            const CONFIGURATION_NONE_U16: u16 = CONFIGURATION_NONE as u16;
            const CONFIGURATION_VALUE_U16: u16 = CONFIGURATION_VALUE as u16;

            match (req.recipient, req.request, req.value) {
                (
                    Recipient::Device,
                    Request::CLEAR_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    self.remote_wakeup_enabled = false;
                    xfer.accept()?;
                }

                (Recipient::Endpoint, Request::CLEAR_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.usb
                        .set_stalled(((req.index as u8) & 0x8f).into(), false)?;
                    xfer.accept()?;
                }

                (
                    Recipient::Device,
                    Request::SET_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    self.remote_wakeup_enabled = true;
                    xfer.accept()?;
                }

                (Recipient::Endpoint, Request::SET_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.usb
                        .set_stalled(((req.index as u8) & 0x8f).into(), true)?;
                    xfer.accept()?;
                }

                (Recipient::Device, Request::SET_ADDRESS, 1..=127) => {
                    if U::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                        self.usb.set_device_address(req.value as u8)?;
                        self.device_state = UsbDeviceState::Addressed;
                    } else {
                        self.pending_address = req.value as u8;
                    }
                    xfer.accept()?;
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_VALUE_U16) => {
                    if self.device_state == UsbDeviceState::Configured
                        || self.device_state == UsbDeviceState::Addressed
                    {
                        if self.device_state == UsbDeviceState::Addressed {
                            Config::visit(classes, &mut EnableEndpointVisitor::new(None, Some(0)))?;

                            self.device_state = UsbDeviceState::Configured;
                        }

                        xfer.accept()?;
                    } else {
                        xfer.reject()?;
                    }
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_NONE_U16) => {
                    match self.device_state {
                        UsbDeviceState::Default => {
                            xfer.reject()?;
                        }
                        _ => {
                            Config::visit(classes, &mut EnableEndpointVisitor::new(None, None))?;

                            self.device_state = UsbDeviceState::Addressed;
                            xfer.accept()?;
                        }
                    }
                }

                (Recipient::Interface, Request::SET_INTERFACE, alt_setting) => {
                    let iface = Some(req.index as u8);
                    let alt_setting = alt_setting as u8;

                    Config::visit(classes, &mut EnableEndpointVisitor::new(iface, None))?;
                    Config::visit(
                        classes,
                        &mut EnableEndpointVisitor::new(iface, Some(alt_setting)),
                    )?;

                    // TODO: Should this check the setting was actually valid?

                    for cls in classes.iter_mut() {
                        cls.alt_setting_activated(
                            InterfaceHandle::from_number(req.index as u8),
                            alt_setting,
                        );
                    }

                    xfer.accept()?;
                }

                _ => {}
            }
        }

        if self.control.waiting_for_response() {
            self.control.reject()?;
        }

        Ok(())
    }

    fn get_descriptor(
        config: &DeviceConfig,
        classes: &mut ClassList<'_, U>,
        xfer: ControlIn<U>,
    ) -> Result<()> {
        let req = *xfer.request();

        let (dtype, index) = req.descriptor_type_index();

        fn accept_writer<U: UsbCore>(
            xfer: ControlIn<U>,
            f: impl FnOnce(DescriptorWriter) -> Result<usize>,
        ) -> Result<()> {
            xfer.accept(|buf| f(DescriptorWriter::new(buf)))
        }

        match dtype {
            descriptor_type::BOS => accept_writer(xfer, |w| {
                let mut bw = BosWriter::new(w)?;

                for cls in classes {
                    cls.get_bos_descriptors(&mut bw)?;
                }

                bw.finish()
            })?,

            descriptor_type::DEVICE => accept_writer(xfer, |mut w| {
                w.write_device(config)?;
                w.finish()
            })?,

            descriptor_type::CONFIGURATION => accept_writer(xfer, |w| {
                let mut cw = ConfigurationDescriptorWriter::new(w, config)?;

                Config::visit(classes, &mut cw)?;

                cw.finish()
            })?,

            descriptor_type::STRING => {
                if index == 0 {
                    accept_writer(xfer, |mut w| {
                        w.write(descriptor_type::STRING, &lang_id::ENGLISH_US.to_le_bytes())?;

                        w.finish()
                    })?;
                } else {
                    let s = match index {
                        allocator::MANUFACTURER_STRING => config.manufacturer,
                        allocator::PRODUCT_STRING => config.product,
                        allocator::SERIAL_NUMBER_STRING => config.serial_number,
                        _ => None,
                    };

                    match s {
                        Some(s) => {
                            accept_writer(xfer, |mut w| {
                                w.write_string(s)?;
                                w.finish()
                            })?;
                        }
                        _ => {
                            let mut visitor = GetStringVisitor {
                                index,
                                xfer: Some(xfer),
                            };

                            let res = Config::visit(classes, &mut visitor);

                            if let Some(xfer) = visitor.xfer.take() {
                                xfer.reject()?;
                            }

                            match res {
                                Ok(_) | Err(UsbError::Break) => {}
                                Err(err) => return Err(err),
                            }
                        }
                    }
                }
            }

            _ => {
                xfer.reject()?;
            }
        }

        Ok(())
    }

    fn reset(&mut self, classes: &mut ClassList<'_, U>) -> Result<()> {
        self.usb.reset()?;

        self.device_state = UsbDeviceState::Default;
        self.remote_wakeup_enabled = false;
        self.pending_address = 0;

        self.control.reset()?;

        for cls in classes {
            cls.reset();
        }

        Ok(())
    }
}

struct EnableEndpointVisitor {
    interface: Option<u8>,
    alt_setting: Option<u8>,
    interface_match: bool,
    current_alt: u8,
}

impl EnableEndpointVisitor {
    fn new(interface: Option<u8>, alt_setting: Option<u8>) -> Self {
        Self {
            interface,
            alt_setting,
            interface_match: false,
            current_alt: 0,
        }
    }

    fn visit_endpoint(
        &mut self,
        endpoint: Option<&mut EndpointCore<impl UsbEndpoint>>,
        config: &EndpointConfig,
    ) -> Result<()> {
        if let Some(endpoint) = endpoint {
            if self.interface_match
                && self
                    .alt_setting
                    .map(|a| a == self.current_alt)
                    .unwrap_or(true)
            {
                if self.alt_setting.is_some() {
                    endpoint.enabled = true;
                    unsafe {
                        endpoint.ep.enable(config)?;
                    }
                } else {
                    endpoint.ep.disable()?;
                    endpoint.enabled = false;
                }
            }
        }

        Ok(())
    }
}

impl<U: UsbCore> ConfigVisitor<U> for EnableEndpointVisitor {
    fn begin_interface(
        &mut self,
        interface: &mut InterfaceHandle,
        _descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        self.interface_match = self.interface.map(|i| i == *interface).unwrap_or(true);

        self.current_alt = 0;

        Ok(())
    }

    fn next_alt_setting(
        &mut self,
        _interface_number: u8,
        _descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        self.current_alt += 1;

        Ok(())
    }

    fn endpoint_out(
        &mut self,
        endpoint: &mut EndpointOut<U>,
        _manual: Option<&[u8]>,
    ) -> Result<()> {
        self.visit_endpoint(endpoint.core.as_mut(), &endpoint.config)
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, _manual: Option<&[u8]>) -> Result<()> {
        self.visit_endpoint(endpoint.core.as_mut(), &endpoint.config)
    }
}

struct GetInterfaceVisitor {
    interface: u8,
    result: Option<u8>,
}

impl GetInterfaceVisitor {
    fn new(interface: u8) -> Self {
        Self {
            interface,
            result: None,
        }
    }

    fn result(&self) -> Option<u8> {
        self.result
    }
}

impl<U: UsbCore> ConfigVisitor<U> for GetInterfaceVisitor {
    fn begin_interface(
        &mut self,
        interface: &mut InterfaceHandle,
        _descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        if *interface == self.interface {
            self.result = Some(interface.alt_setting());
            return Err(UsbError::Break);
        }

        Ok(())
    }
}

struct GetStringVisitor<'p, 'r, U: UsbCore> {
    index: u8,
    xfer: Option<ControlIn<'p, 'r, U>>,
}

impl<U: UsbCore> ConfigVisitor<U> for GetStringVisitor<'_, '_, U> {
    fn string(&mut self, string: &mut StringHandle, value: &str) -> Result<()> {
        if *string == self.index {
            if let Some(xfer) = self.xfer.take() {
                xfer.accept(|buf| {
                    let mut w = DescriptorWriter::new(buf);
                    w.write_string(value)?;
                    w.finish()
                })?;
            }

            return Err(UsbError::Break);
        }

        Ok(())
    }
}
