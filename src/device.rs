use crate::bus::{InterfaceNumber, PollResult, StringIndex, UsbBus, UsbBusAllocator};
use crate::class::{ControlIn, ControlOut, UsbClass};
use crate::control;
use crate::control_pipe::ControlPipe;
use crate::descriptor::{descriptor_type, lang_id::LangID, BosWriter, DescriptorWriter};
pub use crate::device_builder::{UsbDeviceBuilder, UsbVidPid};
use crate::endpoint::{EndpointAddress, EndpointType};
use crate::{Result, UsbDirection};
use core::convert::TryFrom;

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
    pub extra_lang_ids: Option<&'a [LangID]>,
    pub manufacturer: Option<&'a [&'a str]>,
    pub product: Option<&'a [&'a str]>,
    pub serial_number: Option<&'a [&'a str]>,
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
    pub(crate) fn build<'a>(alloc: &'a UsbBusAllocator<B>, config: Config<'a>) -> UsbDevice<'a, B> {
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
            control: ControlPipe::new(control_out, control_in),
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
                    // Handle EP0-IN conditions first. When both EP0-IN and EP0-OUT have completed,
                    // it is possible that EP0-OUT is a zero-sized out packet to complete the STATUS
                    // phase of the control transfer. We have to process EP0-IN first to update our
                    // internal state properly.
                    if (ep_in_complete & 1) != 0 {
                        let completed = self.control.handle_in_complete();

                        if !B::QUIRK_SET_ADDRESS_BEFORE_STATUS
                            && completed
                            && self.pending_address != 0
                        {
                            self.bus.set_device_address(self.pending_address);
                            self.pending_address = 0;

                            self.device_state = UsbDeviceState::Addressed;
                        }
                    }

                    let req = if (ep_setup & 1) != 0 {
                        self.control.handle_setup()
                    } else if (ep_out & 1) != 0 {
                        self.control.handle_out()
                    } else {
                        None
                    };

                    match req {
                        Some(req) if req.direction == UsbDirection::In => {
                            self.control_in(classes, req)
                        }
                        Some(req) if req.direction == UsbDirection::Out => {
                            self.control_out(classes, req)
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
                                cls.endpoint_setup(EndpointAddress::from_parts(
                                    i,
                                    UsbDirection::Out,
                                ));
                            }
                        } else if (ep_out & bit) != 0 {
                            for cls in classes.iter_mut() {
                                cls.endpoint_out(EndpointAddress::from_parts(i, UsbDirection::Out));
                            }
                        }

                        if (ep_in_complete & bit) != 0 {
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
                self.bus.suspend();
                self.suspended_device_state = Some(self.device_state);
                self.device_state = UsbDeviceState::Suspend;
            }
        }

        false
    }

    fn control_in(&mut self, classes: &mut ClassList<'_, B>, req: control::Request) {
        use crate::control::{Recipient, Request};

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
                    let status: u16 = if self.self_powered { 0x0001 } else { 0x0000 }
                        | if self.remote_wakeup_enabled {
                            0x0002
                        } else {
                            0x0000
                        };

                    let _ = xfer.accept_with(&status.to_le_bytes());
                }

                (Recipient::Interface, Request::GET_STATUS) => {
                    let status: u16 = 0x0000;

                    let _ = xfer.accept_with(&status.to_le_bytes());
                }

                (Recipient::Endpoint, Request::GET_STATUS) => {
                    let ep_addr = ((req.index as u8) & 0x8f).into();

                    let status: u16 = if self.bus.is_stalled(ep_addr) {
                        0x0001
                    } else {
                        0x0000
                    };

                    let _ = xfer.accept_with(&status.to_le_bytes());
                }

                (Recipient::Device, Request::GET_DESCRIPTOR) => {
                    UsbDevice::get_descriptor(&self.config, classes, xfer)
                }

                (Recipient::Device, Request::GET_CONFIGURATION) => {
                    let config = match self.device_state {
                        UsbDeviceState::Configured => CONFIGURATION_VALUE,
                        _ => CONFIGURATION_NONE,
                    };

                    let _ = xfer.accept_with(&config.to_le_bytes());
                }

                (Recipient::Interface, Request::GET_INTERFACE) => {
                    // Reject interface numbers bigger than 255
                    if req.index > core::u8::MAX.into() {
                        let _ = xfer.reject();
                        return;
                    }

                    // Ask class implementations, whether they know the alternate setting
                    // of the interface in question
                    for cls in classes {
                        if let Some(setting) = cls.get_alt_setting(InterfaceNumber(req.index as u8))
                        {
                            let _ = xfer.accept_with(&setting.to_le_bytes());
                            return;
                        }
                    }

                    // If no class returned an alternate setting, return the default value
                    let _ = xfer.accept_with(&DEFAULT_ALTERNATE_SETTING.to_le_bytes());
                }

                _ => (),
            };
        }

        if self.control.waiting_for_response() {
            let _ = self.control.reject();
        }
    }

    fn control_out(&mut self, classes: &mut ClassList<'_, B>, req: control::Request) {
        use crate::control::{Recipient, Request};

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
            const DEFAULT_ALTERNATE_SETTING_U16: u16 = DEFAULT_ALTERNATE_SETTING as u16;

            match (req.recipient, req.request, req.value) {
                (
                    Recipient::Device,
                    Request::CLEAR_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    self.remote_wakeup_enabled = false;
                    let _ = xfer.accept();
                }

                (Recipient::Endpoint, Request::CLEAR_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus
                        .set_stalled(((req.index as u8) & 0x8f).into(), false);
                    let _ = xfer.accept();
                }

                (
                    Recipient::Device,
                    Request::SET_FEATURE,
                    Request::FEATURE_DEVICE_REMOTE_WAKEUP,
                ) => {
                    self.remote_wakeup_enabled = true;
                    let _ = xfer.accept();
                }

                (Recipient::Endpoint, Request::SET_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus
                        .set_stalled(((req.index as u8) & 0x8f).into(), true);
                    let _ = xfer.accept();
                }

                (Recipient::Device, Request::SET_ADDRESS, 1..=127) => {
                    if B::QUIRK_SET_ADDRESS_BEFORE_STATUS {
                        self.bus.set_device_address(req.value as u8);
                        self.device_state = UsbDeviceState::Addressed;
                    } else {
                        self.pending_address = req.value as u8;
                    }
                    let _ = xfer.accept();
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_VALUE_U16) => {
                    self.device_state = UsbDeviceState::Configured;
                    let _ = xfer.accept();
                }

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_NONE_U16) => {
                    match self.device_state {
                        UsbDeviceState::Default => {
                            let _ = xfer.accept();
                        }
                        _ => {
                            self.device_state = UsbDeviceState::Addressed;
                            let _ = xfer.accept();
                        }
                    }
                }

                (Recipient::Interface, Request::SET_INTERFACE, alt_setting) => {
                    // Reject interface numbers and alt settings bigger than 255
                    if req.index > core::u8::MAX.into() || alt_setting > core::u8::MAX.into() {
                        let _ = xfer.reject();
                        return;
                    }

                    // Ask class implementations, whether they accept the alternate interface setting.
                    for cls in classes {
                        if cls.set_alt_setting(InterfaceNumber(req.index as u8), alt_setting as u8)
                        {
                            let _ = xfer.accept();
                            return;
                        }
                    }

                    // Default behaviour, if no class implementation accepted the alternate setting.
                    if alt_setting == DEFAULT_ALTERNATE_SETTING_U16 {
                        let _ = xfer.accept();
                    } else {
                        let _ = xfer.reject();
                    }
                }

                _ => {
                    let _ = xfer.reject();
                    return;
                }
            }
        }

        if self.control.waiting_for_response() {
            let _ = self.control.reject();
        }
    }

    fn get_descriptor(config: &Config, classes: &mut ClassList<'_, B>, xfer: ControlIn<B>) {
        let req = *xfer.request();

        let (dtype, index) = req.descriptor_type_index();

        fn accept_writer<B: UsbBus>(
            xfer: ControlIn<B>,
            f: impl FnOnce(&mut DescriptorWriter) -> Result<()>,
        ) {
            let _ = xfer.accept(|buf| {
                let mut writer = DescriptorWriter::new(buf);
                f(&mut writer)?;
                Ok(writer.position())
            });
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
            }),

            descriptor_type::DEVICE => accept_writer(xfer, |w| w.device(config)),

            descriptor_type::CONFIGURATION => accept_writer(xfer, |w| {
                w.configuration(config)?;

                for cls in classes {
                    cls.get_configuration_descriptors(w)?;
                    w.end_class();
                }

                w.end_configuration();

                Ok(())
            }),

            descriptor_type::STRING => match index {
                // first STRING Request
                0 => {
                    if let Some(extra_lang_ids) = config.extra_lang_ids {
                        let mut lang_id_bytes = [0u8; 32];

                        lang_id_bytes
                            .chunks_exact_mut(2)
                            .zip([LangID::EN_US].iter().chain(extra_lang_ids.iter()))
                            .for_each(|(buffer, lang_id)| {
                                buffer.copy_from_slice(&u16::from(lang_id).to_le_bytes());
                            });

                        accept_writer(xfer, |w| {
                            w.write(
                                descriptor_type::STRING,
                                &lang_id_bytes[0..(1 + extra_lang_ids.len()) * 2],
                            )
                        })
                    } else {
                        accept_writer(xfer, |w| {
                            w.write(
                                descriptor_type::STRING,
                                &u16::from(LangID::EN_US).to_le_bytes(),
                            )
                        })
                    }
                }

                // rest STRING Requests
                _ => {
                    let s = match LangID::try_from(req.index) {
                        Err(_err) => {
                            #[cfg(feature = "defmt")]
                            defmt::warn!(
                                "Receive unknown LANGID {:#06X}, reject the request",
                                _err.number
                            );
                            None
                        }

                        Ok(req_lang_id) => {
                            if index <= 3 {
                                // for Manufacture, Product and Serial

                                // construct the list of lang_ids full supported by device
                                let mut lang_id_list: [Option<LangID>; 16] = [None; 16];
                                match config.extra_lang_ids {
                                    None => lang_id_list[0] = Some(LangID::EN_US),
                                    Some(extra_lang_ids) => {
                                        lang_id_list
                                            .iter_mut()
                                            .zip(
                                                [LangID::EN_US].iter().chain(extra_lang_ids.iter()),
                                            )
                                            .for_each(|(item, lang_id)| *item = Some(*lang_id));
                                    }
                                };

                                let position =
                                    lang_id_list.iter().fuse().position(|list_lang_id| {
                                        matches!(*list_lang_id, Some(list_lang_id) if req_lang_id == list_lang_id)
                                    });
                                #[cfg(feature = "defmt")]
                                if position.is_none() {
                                    // Since we construct the list of full supported lang_ids previously,
                                    // we can safely reject requests which ask for other lang_id.
                                    defmt::warn!(
                                        "Receive unknown LANGID {:#06X}, reject the request",
                                        req_lang_id
                                    );
                                }
                                position.and_then(|lang_id_list_index| {
                                    match index {
                                        1 => config.manufacturer,
                                        2 => config.product,
                                        3 => config.serial_number,
                                        _ => unreachable!(),
                                    }
                                    .map(|str_list| str_list[lang_id_list_index])
                                })
                            } else {
                                // for other custom STRINGs

                                let index = StringIndex::new(index);
                                classes
                                    .iter()
                                    .find_map(|cls| cls.get_string(index, req_lang_id))
                            }
                        }
                    };

                    if let Some(s) = s {
                        accept_writer(xfer, |w| w.string(s));
                    } else {
                        let _ = xfer.reject();
                    }
                }
            },

            _ => {
                let _ = xfer.reject();
            }
        }
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
