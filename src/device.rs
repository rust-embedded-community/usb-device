use heapless;
use crate::{Result, UsbDirection};
use crate::bus::{UsbBusAllocator, UsbBus, PollResult, StringIndex};
use crate::descriptor::{DescriptorWriter, descriptor_type, lang_id};
use crate::endpoint::{EndpointType, EndpointAddress};
use crate::control;
use crate::class::{UsbClass, ControlIn, ControlOut};
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
pub struct UsbDevice<'a, B: UsbBus> {
    bus: &'a B,
    config: Config<'a, B>,
    control: control::ControlPipe<'a, B>,
    device_state: UsbDeviceState,
    remote_wakeup_enabled: bool,
    self_powered: bool,
    pending_address: u8,
}

//#[derive(Copy, Clone)]
pub(crate) struct Config<'a, B: UsbBus> {
    pub classes: heapless::Vec<&'a dyn UsbClass<B>, heapless::consts::U8>,
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer: Option<&'a str>,
    pub product: Option<&'a str>,
    pub serial_number: Option<&'a str>,
    pub self_powered: bool,
    pub supports_remote_wakeup: bool,
    pub max_power: u8,
}

impl<B: UsbBus> Clone for Config<'_, B> {
    fn clone(&self) -> Self {
        Config {
            classes: {
                let mut c = heapless::Vec::new();
                c.extend_from_slice(&self.classes).unwrap();
                c
            },
            ..*self
        }
    }
}

pub const CONFIGURATION_VALUE: u8 = 1;

pub const DEFAULT_ALTERNATE_SETTING: u8 = 0;

impl<B: UsbBus> UsbDevice<'_, B> {
    /// Creates a [`UsbDeviceBuilder`] for constructing a new instance.
    pub fn new<'a>(
        bus: &'a UsbBusAllocator<B>,
        vid_pid: UsbVidPid,
        classes: &[&'a dyn UsbClass<B>]) -> UsbDeviceBuilder<'a, B>
    {
        UsbDeviceBuilder::new(bus, vid_pid, classes)
    }

    pub(crate) fn build<'a>(alloc: &'a UsbBusAllocator<B>, config: Config<'a, B>)
        -> UsbDevice<'a, B>
    {
        let control_out = alloc.alloc(Some(0.into()), EndpointType::Control,
            config.max_packet_size_0 as u16, 0).expect("failed to alloc control endpoint");

        let control_in = alloc.alloc(Some(0.into()), EndpointType::Control,
            config.max_packet_size_0 as u16, 0).expect("failed to alloc control endpoint");

        let bus = alloc.freeze();

        let mut dev = UsbDevice {
            bus,
            config,
            control: control::ControlPipe::new(control_out, control_in),
            device_state: UsbDeviceState::Default,
            remote_wakeup_enabled: false,
            self_powered: false,
            pending_address: 0,
        };

        dev.reset();

        dev
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

    /// Forces a reset on the UsbBus.
    pub fn force_reset(&mut self) -> Result<()> {
        self.bus.force_reset()
    }

    /// Polls the [`UsbBus`] for new events and dispatches them accordingly. Returns true if one of
    /// the classes may have data available for reading or be ready for writing, false otherwise.
    /// This should be called periodically as often as possible for the best data rate, or
    /// preferably from an interrupt handler. Must be called at least one every 10 milliseconds to
    /// be USB compliant.
    pub fn poll<'t>(&'t mut self) -> bool {
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
            PollResult::Reset => self.reset(),
            PollResult::Data { ep_out, ep_in_complete, ep_setup } => {
                // Combine bit fields for quick tests
                let mut eps = ep_out | ep_in_complete | ep_setup;

                // Pending events for endpoint 0?
                if (eps & 1) != 0 {
                    let xfer = if (ep_setup & 1) != 0 {
                        self.control.handle_setup()
                    } else if (ep_out & 1) != 0 {
                        self.control.handle_out()
                    } else {
                        None
                    };

                    match xfer {
                        Some(control::TransferDirection::In) => self.control_in(),
                        Some(control::TransferDirection::Out) => self.control_out(),
                        _ => (),
                    };

                    if (ep_in_complete & 1) != 0 {
                        let completed = self.control.handle_in_complete();

                        if completed && self.pending_address != 0 {
                            self.bus.set_device_address(self.pending_address);
                            self.pending_address = 0;

                            self.device_state = UsbDeviceState::Addressed;
                        }
                    }

                    eps &= !1;
                }

                // Pending events for other endpoints?
                if eps != 0 {
                    let mut bit = 2u16;

                    for i in 1..MAX_ENDPOINTS {
                        if (ep_setup & bit) != 0 {
                            for cls in &self.config.classes {
                                cls.endpoint_setup(
                                    EndpointAddress::from_parts(i, UsbDirection::Out));
                            }
                        } else if (ep_out & bit) != 0 {
                            for cls in &self.config.classes {
                                cls.endpoint_out(
                                    EndpointAddress::from_parts(i, UsbDirection::Out));
                            }
                        }

                        if (ep_in_complete & bit) != 0 {
                            for cls in &self.config.classes {
                                cls.endpoint_in_complete(
                                    EndpointAddress::from_parts(i, UsbDirection::In));
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

    fn control_in(&mut self) {
        use crate::control::{Request, Recipient};

        let req = *self.control.request();
        let mut ctrl = Some(&mut self.control);

        for cls in &self.config.classes {
            cls.control_in(ControlIn::new(&mut ctrl));

            if ctrl.is_none() {
                return;
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlIn::new(&mut ctrl);

            match (req.recipient, req.request) {
                (Recipient::Device, Request::GET_STATUS) => {
                    let status: u16 = 0x0000
                        | if self.self_powered { 0x0001 } else { 0x0000 }
                        | if self.remote_wakeup_enabled { 0x0002 } else { 0x0000 };

                    xfer.accept_with(&status.to_le_bytes()).unwrap();
                },

                (Recipient::Interface, Request::GET_STATUS) => {
                    let status: u16 = 0x0000;

                    xfer.accept_with(&status.to_le_bytes()).unwrap();
                },

                (Recipient::Endpoint, Request::GET_STATUS) => {
                    let ep_addr = ((req.index as u8) & 0x8f).into();

                    let status: u16 = 0x0000
                        | if self.bus.is_stalled(ep_addr) { 0x0001 } else { 0x0000 };

                    xfer.accept_with(&status.to_le_bytes()).unwrap();
                },

                (Recipient::Device, Request::GET_DESCRIPTOR)
                    => UsbDevice::get_descriptor(&self.config, xfer).unwrap(),

                (Recipient::Device, Request::GET_CONFIGURATION) => {
                    xfer.accept_with(&CONFIGURATION_VALUE.to_le_bytes()).unwrap();
                },

                (Recipient::Interface, Request::GET_INTERFACE) => {
                    // TODO: change when alternate settings are implemented
                    xfer.accept_with(&DEFAULT_ALTERNATE_SETTING.to_le_bytes()).unwrap();
                },

                _ => (),
            };
        }

        if let Some(ctrl) = ctrl {
            ctrl.reject().unwrap();
        }
    }

    fn control_out(&mut self) {
        use crate::control::{Request, Recipient};

        let req = *self.control.request();
        let mut ctrl = Some(&mut self.control);

        for cls in &self.config.classes {
            cls.control_out(ControlOut::new(&mut ctrl));

            if ctrl.is_none() {
                return;
            }
        }

        if req.request_type == control::RequestType::Standard {
            let xfer = ControlOut::new(&mut ctrl);

            const CONFIGURATION_VALUE_U16: u16 = CONFIGURATION_VALUE as u16;
            const DEFAULT_ALTERNATE_SETTING_U16: u16 = DEFAULT_ALTERNATE_SETTING as u16;

            match (req.recipient, req.request, req.value) {
                (Recipient::Device, Request::CLEAR_FEATURE, Request::FEATURE_DEVICE_REMOTE_WAKEUP) => {
                    self.remote_wakeup_enabled = false;
                    xfer.accept().unwrap();
                },

                (Recipient::Endpoint, Request::CLEAR_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus.set_stalled(((req.index as u8) & 0x8f).into(), false);
                    xfer.accept().unwrap();
                },

                (Recipient::Device, Request::SET_FEATURE, Request::FEATURE_DEVICE_REMOTE_WAKEUP) => {
                    self.remote_wakeup_enabled = true;
                    xfer.accept().unwrap();
                },

                (Recipient::Endpoint, Request::SET_FEATURE, Request::FEATURE_ENDPOINT_HALT) => {
                    self.bus.set_stalled(((req.index as u8) & 0x8f).into(), true);
                    xfer.accept().unwrap();
                },

                (Recipient::Device, Request::SET_ADDRESS, 1..=127) => {
                    self.pending_address = req.value as u8;
                    xfer.accept().unwrap();
                },

                (Recipient::Device, Request::SET_CONFIGURATION, CONFIGURATION_VALUE_U16) => {
                    self.device_state = UsbDeviceState::Configured;
                    xfer.accept().unwrap();
                },

                (Recipient::Interface, Request::SET_INTERFACE, DEFAULT_ALTERNATE_SETTING_U16) => {
                    // TODO: do something when alternate settings are implemented
                    xfer.accept().unwrap();
                },

                _ => { xfer.reject().unwrap(); return; },
            }
        }

        if let Some(ctrl) = ctrl {
            ctrl.reject().unwrap();
        }
    }

    fn get_descriptor(config: &Config<B>, xfer: ControlIn<B>) -> Result<()>{
        let req = *xfer.request();

        let (dtype, index) = get_descriptor_type_index(req.value);

        fn accept_writer<B: UsbBus>(
            xfer: ControlIn<B>,
            f: impl FnOnce(&mut DescriptorWriter) -> Result<()>) -> Result<()>
        {
            xfer.accept(|buf| {
                let mut writer = DescriptorWriter::new(buf);
                f(&mut writer)?;
                Ok(writer.position())
            })
        }

        match dtype {
            descriptor_type::DEVICE => accept_writer(xfer, |w| w.device(config)),

            descriptor_type::CONFIGURATION => accept_writer(xfer, |w| {
                w.configuration(config)?;

                for cls in &config.classes {
                    cls.get_configuration_descriptors(w)?;
                    w.end_class();
                }

                w.end_configuration();

                Ok(())
            }),

            descriptor_type::STRING => {
                if index == 0 {
                    accept_writer(xfer, |w|
                        w.write(
                            descriptor_type::STRING,
                            &lang_id::ENGLISH_US.to_le_bytes()))
                } else {
                    let s = match index {
                        1 => config.manufacturer,
                        2 => config.product,
                        3 => config.serial_number,
                        _ => {
                            let index = StringIndex::new(index);
                            let lang_id = req.index;

                            config.classes.iter()
                                .filter_map(|cls| cls.get_string(index, lang_id))
                                .nth(0)
                        },
                    };

                    if let Some(s) = s {
                        accept_writer(xfer, |w| w.string(s))
                    } else {
                        xfer.reject()
                    }
                }
            },

            _ => xfer.reject(),
        }
    }

    fn reset(&mut self) {
        self.bus.reset();

        self.device_state = UsbDeviceState::Default;
        self.remote_wakeup_enabled = false;
        self.pending_address = 0;

        self.control.reset();

        for cls in &self.config.classes {
            cls.reset().unwrap();
        }
    }
}

/// Gets the descriptor type and value from the value field of a GET_DESCRIPTOR request
fn get_descriptor_type_index(value: u16) -> (u8, u8) {
    ((value >> 8) as u8, value as u8)
}
