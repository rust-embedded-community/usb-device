use crate::bus::{InterfaceNumber, PollResult, StringIndex, UsbBus, UsbBusAllocator};
use crate::class::{ControlIn, ControlOut, UsbClass};
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::descriptor::{descriptor_type, lang_id::LangID, BosWriter, DescriptorWriter};
pub use crate::device_builder::{StringDescriptors, UsbDeviceBuilder, UsbVidPid};
use crate::endpoint::{EndpointAddress, EndpointType};
use crate::{Result, UsbDirection};

/// The global state of the USB device.
///
/// In general class traffic is only possible in the `Configured` state.
#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

/// Usb spec revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum UsbRev {
    /// USB 1.0 compliance
    Usb100 = 0x100,
    /// USB 1.1 compliance
    Usb110 = 0x110,
    /// USB 2.0 compliance
    Usb200 = 0x200,
    /// USB 2.1 compliance.
    ///
    /// Typically adds support for BOS requests.
    Usb210 = 0x210,
}

/// A USB device consisting of one or more device classes.
pub struct UsbDevice<'a, B: UsbBus> {
    bus: &'a B,
    config: Config<'a>,
    control: ControlPipe<'a, B>,
    device_state: UsbDeviceState,
    remote_wakeup_enabled: bool,
    self_powered: bool,
    suspended_device_state: Option<UsbDeviceState>,
    pending_address: u8,
}

pub(crate) struct Config<'a> {
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usb_rev: UsbRev,
    pub device_release: u16,
    pub string_descriptors: heapless::Vec<StringDescriptors<'a>, 16>,
    pub self_powered: bool,
    pub supports_remote_wakeup: bool,
    pub composite_with_iads: bool,
    pub max_power: u8,
}

/// The bConfiguration value for the not configured state.
pub const CONFIGURATION_NONE: u8 = 0;

/// The bConfiguration value for the single configuration supported by this device.
pub const CONFIGURATION_VALUE: u8 = 1;

/// The default value for bAlternateSetting for all interfaces.
pub const DEFAULT_ALTERNATE_SETTING: u8 = 0;

type ClassList<'a, B> = [&'a mut dyn UsbClass<B>];

impl<B: UsbBus> UsbDevice<'_, B> {
    pub(crate) fn build<'a>(
        alloc: &'a UsbBusAllocator<B>,
        config: Config<'a>,
        control_buffer: &'a mut [u8],
    ) -> UsbDevice<'a, B> {
        let control_out = alloc
            .alloc(
                Some(0x00.into()),
                EndpointType::Control,
                config.max_packet_size_0 as u16,
                0,
            )
            .expect("failed to alloc control endpoint");

        let control_in = alloc
            .alloc(
                Some(0x80.into()),
                EndpointType::Control,
                config.max_packet_size_0 as u16,
                0,
            )
            .expect("failed to alloc control endpoint");

        let bus = alloc.freeze();

        UsbDevice {
            bus,
            config,
            control: ControlPipe::new(control_buffer, control_out, control_in),
            device_state: UsbDeviceState::Default,
            remote_wakeup_enabled: false,
            self_powered: false,
            suspended_device_state: None,
            pending_address: 0,
        }
    }

    /// Gets a reference to the [`UsbBus`] implementation used by this `UsbDevice`. You can use this
    /// to call platform-specific methods on the `UsbBus`.
    ///
    /// While it is also possible to call the standard `UsbBus` trait methods through this
    /// reference, this is not recommended as it can cause the device to misbehave.
    pub fn bus(&self) -> &B {
        self.bus
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

    /// Simulates a disconnect from the USB bus, causing the host to reset and re-enumerate the
    /// device.
    ///
    /// Mostly useful for development. Calling this at the start of your program ensures that the
    /// host re-enumerates your device after a new program has been flashed.
    pub fn force_reset(&mut self) -> Result<()> {
        self.bus.force_reset()
    }

    /// Polls the [`UsbBus`] for new events and dispatches them to the provided classes. Returns
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
    pub fn poll(&mut self, classes: &mut ClassList<'_, B>) -> bool {
        let pr = self.bus.poll();

        if self.device_state == UsbDeviceState::Suspend {
            match pr {
                PollResult::Suspend | PollResult::None => {
                    return false;
                }
                _ => {
                    self.bus.resume();

                    self.device_state = self
                        .suspended_device_state
                        .expect("Unknown state before suspend");
                    self.suspended_device_state = None;
                }
            }
        }

        match pr {
            PollResult::None => {}
            PollResult::Reset => self.reset(classes),
            PollResult::Data {
                ep_out,
                ep_in_complete,
                ep_setup,
            } => {
                // Combine bit fields for quick tests
                let mut eps = ep_out | ep_in_complete | ep_setup;

                // Pending events for endpoint 0?
                if (eps & 1) != 0 {
                    usb_debug!(
                        "EP0: setup={}, in_complete={}, out={}",
                        ep_setup & 1,
                        ep_in_complete & 1,
                        ep_out & 1
                    );

                    let req = if (ep_setup & 1) != 0 {
                        self.control.handle_setup()
                    } else if (ep_out & 1) != 0 {
                        match self.control.handle_out() {
                            Ok(req) => req,
                            Err(_err) => {
                                // TODO: Propagate error out of `poll()`
                                usb_debug!("Failed to handle EP0: {:?}", _err);
                                None
                            }
                        }
                    } else {
                        None
                    };

                    match req {
                        Some(req) if req.direction == UsbDirection::In => {
                            if let Err(_err) = self.control_in(classes, req) {
                                // TODO: Propagate error out of `poll()`
                                usb_debug!("Failed to handle input control request: {:?}", _err);
                            }
                        }
                        Some(req) if req.direction == UsbDirection::Out => {
                            if let Err(_err) = self.control_out(classes, req) {
                                // TODO: Propagate error out of `poll()`
                                usb_debug!("Failed to handle output control request: {:?}", _err);
                            }
                        }

                        None if ((ep_in_complete & 1) != 0) => {
                            // We only handle EP0-IN completion if there's no other request being
                            // processed. EP0-IN tokens may be issued due to completed STATUS
                            // phases of the control transfer. If we just got a SETUP packet or
                            // an OUT token, we can safely ignore the IN-COMPLETE indication and
                            // continue with the next transfer.
                            let completed = match self.control.handle_in_complete() {
                                Ok(completed) => completed,
                                Err(_err) => {
                                    // TODO: Propagate this out of `poll()`
                                    usb_debug!(
                                        "Failed to process control-input complete: {:?}",
                                        _err
                                    );
                                    false
                                }
                            };

                            if !B::QUIRK_SET_ADDRESS_BEFORE_STATUS
                                && completed
                                && self.pending_address != 0
                            {
                                self.bus.set_device_address(self.pending_address);
                                self.pending_address = 0;

                                self.device_state = UsbDeviceState::Addressed;
                            }
                        }

                        _ => (),
                    };

                    eps &= !1;
                }

                // Pending events for other endpoints?
                if eps != 0 {
                    let mut bit = 2u16;

                    for i in 1..MAX_ENDPOINTS {
                        if (ep_setup & bit) != 0 {
                            for cls in classes.iter_mut() {
                                usb_trace!("Handling EP{}-SETUP", i);
                                cls.endpoint_setup(EndpointAddress::from_parts(
                                    i,
                                    UsbDirection::Out,
                                ));
                            }
                        } else if (ep_out & bit) != 0 {
                            usb_trace!("Handling EP{}-OUT", i);
                            for cls in classes.iter_mut() {
                                cls.endpoint_out(EndpointAddress::from_parts(i, UsbDirection::Out));
                            }
                        }

                        if (ep_in_complete & bit) != 0 {
                            usb_trace!("Handling EP{}-IN", i);
                            for cls in classes.iter_mut() {
                                cls.endpoint_in_complete(EndpointAddress::from_parts(
                                    i,
                                    UsbDirection::In,
                                ));
                            }
                        }

                        eps &= !bit;

                        if eps == 0 {
                            // No more pending events for higher endpoints
                            break;
                        }

                        bit <<= 1;
                    }
                }

                for cls in classes.iter_mut() {
                    cls.poll();
                }

                return true;
            }
            PollResult::Resume => {}
            PollResult::Suspend => {
                usb_debug!("Suspending bus");
                self.bus.suspend();
                self.suspended_device_state = Some(self.device_state);
                self.device_state = UsbDeviceState::Suspend;
            }
        }

        false
    }

    fn control_in(&mut self, classes: &mut ClassList<'_, B>, req: control::Request) -> Result<()> {
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
                    usb_trace!("Processing Device::GetStatus");
                    let status: u16 = if self.self_powered { 0x0001 } else { 0x0000 }
                        | if self.remote_wakeup_enabled {
                            0x0002
                        } else {
                            0x0000
                        };

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Interface, Request::GET_STATUS) => {
                    usb_trace!("Processing Interface::GetStatus");
                    let status: u16 = 0x0000;

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Endpoint, Request::GET_STATUS) => {
                    usb_trace!("Processing EP::GetStatus");
                    let ep_addr = ((req.index as u8) & 0x8f).into();

                    let status: u16 = if self.bus.is_stalled(ep_addr) {
                        0x0001
                    } else {
                        0x0000
                    };

                    xfer.accept_with(&status.to_le_bytes())?;
                }

                (Recipient::Device, Request::GET_DESCRIPTOR) => {
                    usb_trace!("Processing Device::GetDescriptor");
                    UsbDevice::get_descriptor(&self.config, classes, xfer)?;
                }

                (Recipient::Device, Request::GET_CONFIGURATION) => {
                    usb_trace!("Processing Device::GetConfiguration");
                    let config = match self.device_state {
                        UsbDeviceState::Configured => CONFIGURATION_VALUE,
                        _ => CONFIGURATION_NONE,
                    };

                    xfer.accept_with(&config.to_le_bytes())?;
                }

                (Recipient::Interface, Request::GET_INTERFACE) => {
                    usb_trace!("Processing Interface::GetInterface");
                    // Reject interface numbers bigger than 255
                    if req.index > core::u8::MAX.into() {
                        return xfer.reject();
                    }

                    // Ask class implementations, whether they know the alternate setting
                    // of the interface in question
                    for cls in classes {
                        if let Some(setting) = cls.get_alt_setting(InterfaceNumber(req.index as u8))
                        {
                            return xfer.accept_with(&setting.to_le_bytes());
                        }
                    }

                    // If no class returned an alternate setting, return the default value
                    xfer.accept_with(&DEFAULT_ALTERNATE_SETTING.to_le_bytes())?;
                }

                _ => {}
            };
        }

        if self.control.waiting_for_response() {
            usb_debug!("Rejecting control transfer because we were waiting for a response");
            self.control.reject()?;
        }

        Ok(())
    }

    fn control_out(&mut self, classes: &mut ClassList<'_, B>, req: control::Request) -> Result<()> {
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
            const DEFAULT_ALTERNATE_SETTING_U16: u16 = DEFAULT_ALTERNATE_SETTING as u16;

            match (req.recipient, req.request, req.value) {
                (
                    Recipient::Device,
                    Request::CLEAR_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    usb_debug!("Remote wakeup disabled");
                    self.remote_wakeup_enabled = false;
                    xfer.accept()?;
                }

                (Recipient::Endpoint, Request::CLEAR_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    usb_debug!("EP{} halt removed", req.index & 0x8f);
                    self.bus
                        .set_stalled(((req.index as u8) & 0x8f).into(), false);
                    xfer.accept()?;
                }

                (
                    Recipient::Device,
                    Request::SET_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    usb_debug!("Remote wakeup enabled");
                    self.remote_wakeup_enabled = true;
                    xfer.accept()?;
                }

                (Recipient::Endpoint, Request::SET_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    usb_debug!("EP{} halted", req.index & 0x8f);
                    self.bus
                        .set_stalled(((req.index as u8) & 0x8f).into(), true);
                    xfer.accept()?;
                }

                (Recipient::Device, Request::SET_ADDRESS, 1..=127) => {
                    usb_debug!("Setting device address to {}", req.value);
                    if B::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                        self.bus.set_device_address(req.value as u8);
                        self.device_state = UsbDeviceState::Addressed;
                    } else {
                        self.pending_address = req.value as u8;
                    }
                    xfer.accept()?;
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_VALUE_U16) => {
                    usb_debug!("Device configured");
                    self.device_state = UsbDeviceState::Configured;
                    xfer.accept()?;
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_NONE_U16) => {
                    usb_debug!("Device deconfigured");
                    match self.device_state {
                        UsbDeviceState::Default => {
                            xfer.accept()?;
                        }
                        _ => {
                            self.device_state = UsbDeviceState::Addressed;
                            xfer.accept()?;
                        }
                    }
                }

                (Recipient::Interface, Request::SET_INTERFACE, alt_setting) => {
                    // Reject interface numbers and alt settings bigger than 255
                    if req.index > core::u8::MAX.into() || alt_setting > core::u8::MAX.into() {
                        xfer.reject()?;
                        return Ok(());
                    }

                    // Ask class implementations, whether they accept the alternate interface setting.
                    for cls in classes {
                        if cls.set_alt_setting(InterfaceNumber(req.index as u8), alt_setting as u8)
                        {
                            xfer.accept()?;
                            return Ok(());
                        }
                    }

                    // Default behaviour, if no class implementation accepted the alternate setting.
                    if alt_setting == DEFAULT_ALTERNATE_SETTING_U16 {
                        usb_debug!("Accepting unused alternate settings");
                        xfer.accept()?;
                    } else {
                        usb_debug!("Rejecting unused alternate settings");
                        xfer.reject()?;
                    }
                }

                _ => {
                    xfer.reject()?;
                    return Ok(());
                }
            }
        }

        if self.control.waiting_for_response() {
            usb_debug!("Rejecting control transfer due to waiting response");
            self.control.reject()?;
        }

        Ok(())
    }

    fn get_descriptor(
        config: &Config,
        classes: &mut ClassList<'_, B>,
        xfer: ControlIn<B>,
    ) -> Result<()> {
        let req = *xfer.request();

        let (dtype, index) = req.descriptor_type_index();

        fn accept_writer<B: UsbBus>(
            xfer: ControlIn<B>,
            f: impl FnOnce(&mut DescriptorWriter) -> Result<()>,
        ) -> Result<()> {
            xfer.accept(|buf| {
                let mut writer = DescriptorWriter::new(buf);
                f(&mut writer)?;
                Ok(writer.position())
            })?;

            Ok(())
        }

        match dtype {
            descriptor_type::BOS if config.usb_rev > UsbRev::Usb200 => accept_writer(xfer, |w| {
                let mut bw = BosWriter::new(w);
                bw.bos()?;

                for cls in classes {
                    cls.get_bos_descriptors(&mut bw)?;
                }

                bw.end_bos();

                Ok(())
            })?,

            descriptor_type::DEVICE => accept_writer(xfer, |w| w.device(config))?,

            descriptor_type::CONFIGURATION => accept_writer(xfer, |w| {
                w.configuration(config)?;

                for cls in classes {
                    cls.get_configuration_descriptors(w)?;
                    w.end_class();
                }

                w.end_configuration();

                Ok(())
            })?,

            descriptor_type::STRING => match index {
                // first STRING Request
                0 => {
                    let mut lang_id_bytes = [0u8; 32];
                    for (lang, buf) in config
                        .string_descriptors
                        .iter()
                        .zip(lang_id_bytes.chunks_exact_mut(2))
                    {
                        buf.copy_from_slice(&u16::from(lang.id).to_le_bytes());
                    }
                    accept_writer(xfer, |w| {
                        w.write(
                            descriptor_type::STRING,
                            &lang_id_bytes[..config.string_descriptors.len() * 2],
                        )
                    })?;
                }

                // rest STRING Requests
                _ => {
                    let lang_id = LangID::from(req.index);

                    let string = match index {
                        // Manufacturer, product, and serial are handled directly here.
                        1..=3 => {
                            let Some(lang) = config
                                .string_descriptors
                                .iter()
                                .find(|lang| lang.id == lang_id)
                            else {
                                xfer.reject()?;
                                return Ok(());
                            };

                            match index {
                                1 => lang.manufacturer,
                                2 => lang.product,
                                3 => lang.serial,
                                _ => unreachable!(),
                            }
                        }
                        _ => {
                            let index = StringIndex::new(index);
                            classes
                                .iter()
                                .find_map(|cls| cls.get_string(index, lang_id))
                        }
                    };

                    if let Some(string_descriptor) = string {
                        accept_writer(xfer, |w| w.string(string_descriptor))?;
                    } else {
                        xfer.reject()?;
                    }
                }
            },

            _ => {
                xfer.reject()?;
            }
        };

        Ok(())
    }

    fn reset(&mut self, classes: &mut ClassList<'_, B>) {
        self.bus.reset();

        self.device_state = UsbDeviceState::Default;
        self.suspended_device_state = None; // We may reset during Suspend
        self.remote_wakeup_enabled = false;
        self.pending_address = 0;

        self.control.reset();

        for cls in classes {
            cls.reset();
        }
    }
}
