use bus::UsbBus;
use control;
use device::{UsbDevice, UsbDeviceState, ControlOutResult, ControlInResult};
use descriptor::{DescriptorWriter, descriptor_type, lang_id};

impl<'a, T: UsbBus + 'a> UsbDevice<'a, T> {
    pub(crate) fn standard_control_out(&self, req: &control::Request, buf: &[u8]) -> ControlOutResult {
        let _ = buf;

        use control::{Recipient, standard_request as sr};

        match (req.recipient, req.request) {
            (_, sr::CLEAR_FEATURE) => {
                // TODO: Actually implement
                ControlOutResult::Ok
            },
            (_, sr::SET_FEATURE) => {
                // TODO: Actually implement
                ControlOutResult::Ok
            },
            (Recipient::Device, sr::SET_ADDRESS) => {
                if req.value > 0 && req.value <= 127 {
                    self.pending_address.set(req.value as u8);
                    ControlOutResult::Ok
                } else {
                    ControlOutResult::Err
                }
            },
            (Recipient::Device, sr::SET_CONFIGURATION) => {
                self.device_state.set(UsbDeviceState::Configured);
                ControlOutResult::Ok
            },
            (Recipient::Interface, sr::SET_INTERFACE) => {
                // TODO: Actually support alternate settings
                if req.value == 0 {
                    ControlOutResult::Ok
                } else {
                    ControlOutResult::Err
                }
            },
            _ => ControlOutResult::Err,
        }
    }

    pub(crate) fn standard_control_in(&self, req: &control::Request, buf: &mut [u8]) -> ControlInResult {
        use control::{Recipient, standard_request as sr};
        match (req.recipient, req.request) {
            (_, sr::GET_STATUS) => {
                // TODO: Actual implement
                buf[..2].copy_from_slice(&[0, 0]);
                ControlInResult::Ok(2)
            },
            (Recipient::Device, sr::GET_DESCRIPTOR) => self.handle_get_descriptor(&req, buf),
            (Recipient::Device, sr::GET_CONFIGURATION) => {
                buf[0] = 0x00;
                ControlInResult::Ok(1)
            },
            (Recipient::Interface, sr::GET_INTERFACE) => {
                // TODO: Actually support alternate settings
                buf[0] = 0x00;
                ControlInResult::Ok(1)
            },
            _ => ControlInResult::Ignore,
        }
    }

    fn handle_get_descriptor(&self, req: &control::Request, buf: &mut [u8]) -> ControlInResult {
        let (dtype, index) = req.descriptor_type_index();

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
                        1, // bConfigurationValue
                        0, // iConfiguration
                        0x80
                            | if self.info.self_powered { 0x40 } else { 0x00 }
                            | if self.info.remote_wakeup { 0x20 } else { 0x00 }, // bmAttributes
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
                    if let Some(s) = self.get_string(index as usize, req.index) {
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

    fn get_string(&self, index: usize, _lang_id: u16) -> Option<&'a str> {
        match index {
            1 => Some(self.info.manufacturer),
            2 => Some(self.info.product),
            3 => Some(self.info.serial_number),
            _ => None
        }
    }
}