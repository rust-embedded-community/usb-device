use core::sync::atomic::Ordering;
use bus::{UsbBus, StringIndex};
use control;
use device::{UsbDevice, UsbDeviceState, ControlOutResult, ControlInResult};
use descriptor::{DescriptorWriter, descriptor_type, lang_id};

const FEATURE_ENDPOINT_HALT: u16 = 0;
const FEATURE_DEVICE_REMOTE_WAKEUP: u16 = 1;

const CONFIGURATION_VALUE: u16 = 1;

const DEFAULT_ALTERNATE_SETTING: u16 = 0;

/// Gets the endpoint index and corresponding bit from an endpoint control request value field
fn get_ep_index_bit(index: u16) -> (u8, u32) {
    let ep: u8 = (index & 0x8f) as u8;
    let bit: u32 = 1u32 << (((ep & 0x80) >> 3) | (ep & 0x0f));

    (ep, bit)
}

/// Gets the descriptor type and value from the value field of a GET_DESCRIPTOR request
fn get_descriptor_type_index(value: u16) -> (u8, u8) {
    ((value >> 8) as u8, value as u8)
}

impl<'a, B: UsbBus + 'a> UsbDevice<'a, B> {
    pub(crate) fn standard_control_out(&self, req: &control::Request, buf: &[u8]) -> ControlOutResult {
        let _ = buf;

        use control::{Recipient, standard_request as sr};

        match (req.recipient, req.request, req.value) {
            (Recipient::Device, sr::CLEAR_FEATURE, FEATURE_DEVICE_REMOTE_WAKEUP) => {
                self.remote_wakeup_enabled.store(false, Ordering::SeqCst);
                ControlOutResult::Ok
            },

            (Recipient::Endpoint, sr::CLEAR_FEATURE, FEATURE_ENDPOINT_HALT) => {
                let (ep, bit) = get_ep_index_bit(req.index);
                self.bus.set_stalled(ep, false);
                ControlOutResult::Ok
            },

            (Recipient::Device, sr::SET_FEATURE, FEATURE_DEVICE_REMOTE_WAKEUP) => {
                self.remote_wakeup_enabled.store(true, Ordering::SeqCst);
                ControlOutResult::Ok
            },

            (Recipient::Endpoint, sr::SET_FEATURE, FEATURE_ENDPOINT_HALT) => {
                let (ep, bit) = get_ep_index_bit(req.index);
                self.bus.set_stalled(ep, true);
                ControlOutResult::Ok
            },

            (Recipient::Device, sr::SET_ADDRESS, 1..=127) => {
                self.pending_address.store(req.value as usize, Ordering::SeqCst);
                ControlOutResult::Ok
            },

            (Recipient::Device, sr::SET_CONFIGURATION, CONFIGURATION_VALUE) => {
                self.set_state(UsbDeviceState::Configured);
                ControlOutResult::Ok
            },

            (Recipient::Interface, sr::SET_INTERFACE, DEFAULT_ALTERNATE_SETTING) => {
                // TODO: change when alternate settings are implemented
                ControlOutResult::Ok
            },

            _ => ControlOutResult::Err,
        }
    }

    pub(crate) fn standard_control_in(&self, req: &control::Request, buf: &mut [u8]) -> ControlInResult {
        use control::{Recipient, standard_request as sr};
        match (req.recipient, req.request) {
            (Recipient::Device, sr::GET_STATUS) => {
                let status: u16 = 0x0000
                    | if self.self_powered.load(Ordering::SeqCst) { 0x0001 } else { 0x0000 }
                    | if self.remote_wakeup_enabled.load(Ordering::SeqCst) { 0x0002 } else { 0x0000 };

                buf[0] = status as u8;
                buf[1] = (status >> 8) as u8;
                ControlInResult::Ok(2)
            },

            (Recipient::Interface, sr::GET_STATUS) => {
                let status: u16 = 0x0000;

                buf[0] = status as u8;
                buf[1] = (status >> 8) as u8;
                ControlInResult::Ok(2)
            },

            (Recipient::Endpoint, sr::GET_STATUS) => {
                let status: u16 = 0x0000
                    | if self.bus.is_stalled(req.index as u8) { 0x0001 } else { 0x0000 };

                buf[0] = status as u8;
                buf[1] = (status >> 8) as u8;
                ControlInResult::Ok(2)
            },

            (Recipient::Device, sr::GET_DESCRIPTOR) => self.handle_get_descriptor(req, buf),

            (Recipient::Device, sr::GET_CONFIGURATION) => {
                buf[0] = CONFIGURATION_VALUE as u8;
                ControlInResult::Ok(1)
            },

            (Recipient::Interface, sr::GET_INTERFACE) => {
                // TODO: change when alternate settings are implemented
                buf[0] = DEFAULT_ALTERNATE_SETTING as u8;
                ControlInResult::Ok(1)
            },

            _ => ControlInResult::Err,
        }
    }

    fn handle_get_descriptor(&self, req: &control::Request, buf: &mut [u8]) -> ControlInResult {
        let (dtype, index) = get_descriptor_type_index(req.value);

        let mut writer = DescriptorWriter::new(buf);

        match dtype {
            descriptor_type::DEVICE => {
                writer.write(
                    descriptor_type::DEVICE,
                    &[
                        0x00, 0x02, // bcdUSB
                        self.info.device_class, // bDeviceClass
                        self.info.device_sub_class, // bDeviceSubClass
                        self.info.device_protocol, // bDeviceProtocol
                        self.info.max_packet_size_0, // bMaxPacketSize0
                        self.info.vendor_id as u8, (self.info.vendor_id >> 8) as u8, // idVendor
                        self.info.product_id as u8, (self.info.product_id >> 8) as u8, // idProduct
                        self.info.device_release as u8, (self.info.device_release >> 8) as u8, // bcdDevice
                        1, // iManufacturer
                        2, // iProduct
                        3, // iSerialNumber
                        1, // bNumConfigurations
                    ]).unwrap();
            },

            descriptor_type::CONFIGURATION => {
                writer.write(
                    descriptor_type::CONFIGURATION,
                    &[
                        0, 0, // wTotalLength (placeholder)
                        0, // bNumInterfaces (placeholder)
                        CONFIGURATION_VALUE as u8, // bConfigurationValue
                        0, // iConfiguration
                        // bmAttributes:
                        0x80
                            | if self.info.self_powered { 0x40 } else { 0x00 }
                            | if self.info.supports_remote_wakeup { 0x20 } else { 0x00 },
                        self.info.max_power // bMaxPower
                    ]).unwrap();

                for cls in self.classes() {
                    cls.get_configuration_descriptors(&mut writer).unwrap();
                }

                let total_length = writer.count();
                let num_interfaces = writer.num_interfaces();

                writer.insert(2, &[total_length as u8, (total_length >> 8) as u8]);

                writer.insert(4, &[num_interfaces]);
            },

            descriptor_type::STRING => {
                if index == 0 {
                    writer.write(
                        descriptor_type::STRING,
                        &[
                            lang_id::ENGLISH_US as u8,
                            (lang_id::ENGLISH_US >> 8) as u8,
                        ]).unwrap();
                } else {
                    if let Some(s) = self.get_string(index, req.index) {
                        writer.write_string(s).unwrap();
                    } else {
                        return ControlInResult::Err;
                    }
                }
            },

            _ => { return ControlInResult::Err; },
        }

        ControlInResult::Ok(writer.count())
    }

    fn get_string(&self, index: u8, lang_id: u16) -> Option<&'a str> {
        match index {
            1 => Some(self.info.manufacturer),
            2 => Some(self.info.product),
            3 => Some(self.info.serial_number),
            _ => {
                let index = StringIndex::new(index);

                for cls in self.classes() {
                    let s = cls.get_string(index, lang_id);

                    if s.is_some() {
                        return s;
                    }
                }

                None
            },
        }
    }
}