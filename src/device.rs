use crate::{Result, UsbDirection};
use crate::allocator::{UsbAllocator, InterfaceHandle, StringHandle, self};
use crate::usbcore::{UsbCore, UsbEndpoint, PollResult};
use crate::class::{UsbClass, ControlIn, ControlOut};
use crate::config::{Config, ConfigVisitor, InterfaceDescriptor};
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::descriptor::{DescriptorWriter, ConfigurationDescriptorWriter, BosWriter, descriptor_type, lang_id};
use crate::endpoint::{EndpointAddress, EndpointConfig, EndpointOut, EndpointIn};
pub use crate::device_builder::{UsbDeviceBuilder, UsbVidPid};

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

// Maximum number of endpoints in one direction. Specified by the USB specification.
const MAX_ENDPOINTS: usize = 16;

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<U: UsbCore> {
    bus: U,
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
    pub(crate) fn build(mut bus: U, config: DeviceConfig, classes: &mut ClassList<U>) -> UsbDevice<U> {
        let mut ep_alloc = bus.create_allocator();

        let control = ControlPipe::new(&mut ep_alloc, config.max_packet_size_0);

        Config::visit(classes, &mut UsbAllocator::new(bus.create_allocator())).expect("Configuration failed");

        bus.enable();

        UsbDevice {
            bus,
            config,
            control,
            device_state: UsbDeviceState::Default,
            remote_wakeup_enabled: false,
            self_powered: false,
            pending_address: 0,
        }
    }

    /// Gets a reference to the [`UsbCore`] implementation used by this `UsbDevice`. You can use this
    /// to call platform-specific methods on the `UsbCore`.
    ///
    /// While it is also possible to call the standard `UsbCore` trait methods through this
    /// reference, it is not recommended as it can cause the device to misbehave.
    pub fn bus(&self) -> &U {
        &self.bus
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
    /// true if one of the classes may have data available for reading or be ready for writing,
    /// false otherwise. This should be called periodically as often as possible for the best data
    /// rate, or preferably from an interrupt handler. Must be called at least once every 10
    /// milliseconds while connected to the USB host to be USB compliant.
    ///
    /// Note: The list of classes passed in must be the same classes in the same order for every
    /// call while the device is configured, or the device may enumerate incorrectly or otherwise
    /// misbehave. The easiest way to do this is to call the `poll` method in only one place in your
    /// code, as follows:
    ///
    /// ``` ignore
    /// usb_dev.poll(&mut [&mut class1, &mut class2]);
    /// ```
    ///
    /// Strictly speaking the list of classes is allowed to change between polls if the device has
    /// been reset, which is indicated by `state` being equal to [`UsbDeviceState::Default`].
    pub fn poll(&mut self, classes: &mut ClassList<'_, U>) -> bool {
        let pr = self.bus.poll();

        if self.device_state == UsbDeviceState::Suspend {
            match pr {
                PollResult::Suspend | PollResult::None => { return false; },
                _ => {
                    self.bus.resume();
                    self.device_state = UsbDeviceState::Default;
                },
            }
        }

        match pr {
            PollResult::None => { }
            PollResult::Reset => self.reset(classes),
            PollResult::Data { mut ep_out, mut ep_in_complete } => {
                // Handle EP0-IN conditions first. When both EP0-IN and EP0-OUT have completed,
                // it is possible that EP0-OUT is a zero-sized out packet to complete the STATUS
                // phase of the control transfer. We have to process EP0-IN first to update our
                // internal state properly.
                if (ep_in_complete & 1) != 0 {
                    let completed = self.control.handle_in_complete();

                    if !U::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                        if completed && self.pending_address != 0 {
                            self.bus.set_device_address(self.pending_address);
                            self.pending_address = 0;

                            self.device_state = UsbDeviceState::Addressed;
                        }
                    }

                    ep_in_complete &= !1;
                }

                // Handle EP0-OUT second.
                if (ep_out & 1) != 0 {
                    let req = self.control.handle_out();

                    match req {
                        Some(req) if req.direction == UsbDirection::In
                            => self.control_in(classes, req),
                        Some(req) if req.direction == UsbDirection::Out
                            => self.control_out(classes, req),
                        _ => (),
                    };

                    ep_out &= !1;
                }

                // Pending events for other endpoints?

                let mut bit = 2u16;

                for i in 1..MAX_ENDPOINTS {
                    if (ep_out & bit) != 0 {
                        for cls in classes.iter_mut() {
                            cls.endpoint_out(
                                EndpointAddress::from_parts(i as u8, UsbDirection::Out));
                        }
                    }

                    if (ep_in_complete & bit) != 0 {
                        for cls in classes.iter_mut() {
                            cls.endpoint_in_complete(
                                EndpointAddress::from_parts(i as u8, UsbDirection::In));
                        }
                    }

                    ep_out &= !bit;
                    ep_in_complete &= !bit;

                    if ep_out == 0 && ep_in_complete == 0 {
                        // No more pending events for higher endpoints
                        break;
                    }

                    bit <<= 1;
                }

                for cls in classes.iter_mut() {
                    cls.poll();
                }

                return true;
            },
            PollResult::Resume => { }
            PollResult::Suspend => {
                self.bus.suspend();
                self.device_state = UsbDeviceState::Suspend;
            }
        }

        return false;
    }

    fn control_in(&mut self, classes: &mut ClassList<'_, U>, req: control::Request) {
        use crate::control::{Request, Recipient};

        for cls in classes.iter_mut() {
            cls.control_in(ControlIn::new(&mut self.control, &req));

            if !self.control.waiting_for_response() {
                return;
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlIn::new(&mut self.control, &req);

            match (req.recipient, req.request) {
                (Recipient::Device, Request::GET_STATUS) => {
                    let status: u16 = 0x0000
                        | if self.self_powered { 0x0001 } else { 0x0000 }
                        | if self.remote_wakeup_enabled { 0x0002 } else { 0x0000 };

                    xfer.accept_with(&status.to_le_bytes()).ok();
                },

                (Recipient::Interface, Request::GET_STATUS) => {
                    let status: u16 = 0x0000;

                    xfer.accept_with(&status.to_le_bytes()).ok();
                },

                (Recipient::Endpoint, Request::GET_STATUS) => {
                    let ep_addr = ((req.index as u8) & 0x8f).into();

                    let status: u16 = 0x0000
                        | if self.bus.is_stalled(ep_addr) { 0x0001 } else { 0x0000 };

                    xfer.accept_with(&status.to_le_bytes()).ok();
                },

                (Recipient::Device, Request::GET_DESCRIPTOR)
                    => UsbDevice::get_descriptor(&self.config, classes, xfer),

                (Recipient::Device, Request::GET_CONFIGURATION) => {
                    let config = match self.device_state {
                        UsbDeviceState::Configured => CONFIGURATION_VALUE,
                        _ => CONFIGURATION_NONE,
                    };

                    xfer.accept_with(&config.to_le_bytes()).ok();
                },

                (Recipient::Interface, Request::GET_INTERFACE) => {
                    let iface = InterfaceHandle(Some(req.index as u8));

                    // FIXME: Unimplemented

                    xfer.reject().ok();
                },

                _ => (),
            };
        }

        if self.control.waiting_for_response() {
            self.control.reject().ok();
        }
    }

    fn control_out(&mut self, classes: &mut ClassList<'_, U>, req: control::Request) {
        use crate::control::{Request, Recipient};

        for cls in classes.iter_mut() {
            cls.control_out(ControlOut::new(&mut self.control, &req));

            if !self.control.waiting_for_response() {
                return;
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlOut::new(&mut self.control, &req);

            const CONFIGURATION_NONE_U16: u16 = CONFIGURATION_NONE as u16;
            const CONFIGURATION_VALUE_U16: u16 = CONFIGURATION_VALUE as u16;

            match (req.recipient, req.request, req.value) {
                (Recipient::Device, Request::CLEAR_FEATURE, Request::FEATURE_DEVICE_REMOTE_WAKEUP) => {
                    self.remote_wakeup_enabled = false;
                    xfer.accept().ok();
                },

                (Recipient::Endpoint, Request::CLEAR_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus.set_stalled(((req.index as u8) & 0x8f).into(), false);
                    xfer.accept().ok();
                },

                (Recipient::Device, Request::SET_FEATURE, Request::FEATURE_DEVICE_REMOTE_WAKEUP) => {
                    self.remote_wakeup_enabled = true;
                    xfer.accept().ok();
                },

                (Recipient::Endpoint, Request::SET_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus.set_stalled(((req.index as u8) & 0x8f).into(), true);
                    xfer.accept().ok();
                },

                (Recipient::Device, Request::SET_ADDRESS, 1..=127) => {
                    if U::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                        self.bus.set_device_address(req.value as u8);
                        self.device_state = UsbDeviceState::Addressed;
                    } else {
                        self.pending_address = req.value as u8;
                    }
                    xfer.accept().ok();
                },

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_VALUE_U16) => {
                    if self.device_state != UsbDeviceState::Configured {
                        // TODO: report to classes?

                        if Config::visit(
                            classes,
                            &mut EnableEndpointVisitor::new(None, Some(0))).is_ok()
                        {
                            self.device_state = UsbDeviceState::Configured;

                            xfer.accept().ok();
                        } else {
                            xfer.reject().ok();
                        }
                    }
                },

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_NONE_U16) => {
                    match self.device_state {
                        UsbDeviceState::Default => {
                            xfer.reject().ok();
                        },
                        _ => {
                            // TODO: report to classes?

                            if Config::visit(
                                classes,
                                &mut EnableEndpointVisitor::new(None, None)).is_ok()
                            {
                                self.device_state = UsbDeviceState::Addressed;
                                xfer.accept().ok();
                            } else {
                                xfer.reject().ok();
                            }
                        },
                    }
                },

                (Recipient::Interface, Request::SET_INTERFACE, alt_setting) => {
                    let iface = Some(req.index as u8);
                    let alt_setting = alt_setting as u8;

                    if Config::visit(
                        classes,
                        &mut EnableEndpointVisitor::new(
                            iface,
                            Some(alt_setting))).is_ok()
                    {
                        for cls in classes.iter_mut() {
                            cls.alt_setting_activated(InterfaceHandle(iface), alt_setting);
                        }

                        xfer.accept().ok();
                    } else {
                        xfer.reject().ok();
                    }
                },

                _ => { xfer.reject().ok(); return; },
            }
        }

        if self.control.waiting_for_response() {
            self.control.reject().ok();
        }
    }

    fn get_descriptor(config: &DeviceConfig, classes: &mut ClassList<'_, U>, xfer: ControlIn<U>) {
        let req = *xfer.request();

        let (dtype, index) = req.descriptor_type_index();

        fn accept_writer<U: UsbCore>(
            xfer: ControlIn<U>,
            f: impl FnOnce(DescriptorWriter) -> Result<usize>)
        {
            xfer.accept(|buf| {
                f(DescriptorWriter::new(buf))
            }).ok();
        }

        match dtype {
            descriptor_type::BOS => accept_writer(xfer, |w| {
                let mut bw = BosWriter::new(w)?;

                for cls in classes {
                    cls.get_bos_descriptors(&mut bw)?;
                }

                bw.finish()
            }),

            descriptor_type::DEVICE => accept_writer(xfer, |mut w| {
                w.write_device(config)?;
                w.finish()
            }),

            descriptor_type::CONFIGURATION => accept_writer(xfer, |w| {
                let mut cw = ConfigurationDescriptorWriter::new(w, config)?;

                Config::visit(classes, &mut cw)?;

                cw.finish()
            }),

            descriptor_type::STRING => {
                if index == 0 {
                    accept_writer(xfer, |mut w| {
                        w.write(
                            descriptor_type::STRING,
                            &lang_id::ENGLISH_US.to_le_bytes())?;

                        w.finish()
                    });
                } else {
                    let s = match index {
                        allocator::MANUFACTURER_STRING => config.manufacturer,
                        allocator::PRODUCT_STRING => config.product,
                        allocator::SERIAL_NUMBER_STRING => config.serial_number,
                        _ => {
                            let lang_id = req.index;

                            classes.iter()
                                .filter_map(|cls| cls.get_string(StringHandle(Some(index)), lang_id))
                                .next()
                        },
                    };

                    if let Some(s) = s {
                        accept_writer(xfer, |mut w| {
                            w.write_string(s)?;
                            w.finish()
                        });
                    } else {
                        xfer.reject().ok();
                    }
                }
            },

            _ => { xfer.reject().ok(); },
        }
    }

    fn reset(&mut self, classes: &mut ClassList<'_, U>) {
        self.bus.reset();

        self.device_state = UsbDeviceState::Default;
        self.remote_wakeup_enabled = false;
        self.pending_address = 0;

        self.control.reset();

        for cls in classes {
            cls.reset();
        }
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

    fn visit_endpoint(&mut self, endpoint: Option<&mut impl UsbEndpoint>, config: &EndpointConfig) -> Result<()> {
        if let Some(endpoint) = endpoint {
            if self.interface_match
                && self.alt_setting.map(|a| a == self.current_alt).unwrap_or(true)
            {
                if self.alt_setting.is_some() {
                    unsafe { endpoint.enable(config); }
                } else {
                    endpoint.disable();
                }
            }
        }

        Ok(())
    }
}

// TODO: Clean up the Option mess
impl<U: UsbCore> ConfigVisitor<U> for EnableEndpointVisitor {
    fn begin_interface(&mut self, interface: &mut InterfaceHandle, _descriptor: &InterfaceDescriptor) -> Result<()> {
        self.interface_match = self.interface.map(|i| i == interface.into()).unwrap_or(true);
        self.current_alt = 0;

        Ok(())
    }

    fn next_alt_setting(&mut self, _interface: &mut InterfaceHandle, _descriptor: &InterfaceDescriptor) -> Result<()>{
        self.current_alt += 1;

        Ok(())
    }

    fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>, _extra: Option<&[u8]>) -> Result<()> {
        self.visit_endpoint(endpoint.core.as_mut().map(|c| &mut c.ep), &endpoint.config)
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, _extra: Option<&[u8]>) -> Result<()> {
        self.visit_endpoint(endpoint.core.as_mut().map(|c| &mut c.ep), &endpoint.config)
    }
}