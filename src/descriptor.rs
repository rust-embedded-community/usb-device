use crate::allocator::InterfaceHandle;
use crate::config::{ConfigVisitor, InterfaceDescriptor};
use crate::device::{DeviceConfig, IadMode, CONFIGURATION_VALUE};
use crate::endpoint::{EndpointAddress, EndpointConfig, EndpointIn, EndpointOut};
use crate::usbcore::UsbCore;
use crate::{Result, UsbError};

/// Standard descriptor types
#[allow(missing_docs)]
pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTERFACE: u8 = 4;
    pub const ENDPOINT: u8 = 5;
    pub const INTERFACE_ASSOCIATION: u8 = 11;
    pub const BOS: u8 = 15;
    pub const CAPABILITY: u8 = 16;
}

/// String descriptor language IDs.
pub mod lang_id {
    /// English (US)
    ///
    /// Recommended for use as the first language ID for compatibility.
    pub const ENGLISH_US: u16 = 0x0409;
}

/// Standard capability descriptor types
#[allow(missing_docs)]
pub mod capability_type {
    pub const WIRELESS_USB: u8 = 1;
    pub const USB_2_0_EXTENSION: u8 = 2;
    pub const SS_USB_DEVICE: u8 = 3;
    pub const CONTAINER_ID: u8 = 4;
    pub const PLATFORM: u8 = 5;
}

/// A writer for USB descriptors.
pub struct DescriptorWriter<'b> {
    buf: &'b mut [u8],
    pos: usize,
}

impl DescriptorWriter<'_> {
    /// Creates a new descriptor writer that will write to the provided buffer.
    pub fn new(buf: &mut [u8]) -> DescriptorWriter<'_> {
        DescriptorWriter { buf, pos: 0 }
    }

    /// Gets the current position in the buffer, i.e. the number of bytes written so far.
    fn pos(&self) -> usize {
        self.pos
    }

    fn buf(&mut self) -> &mut [u8] {
        self.buf
    }

    /// Writes a USB standard format descriptor to the buffer. The descriptor type and length fields
    /// will be written, followed by the data in `descriptor`.
    pub fn write(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        let length = descriptor.len();

        if (self.pos + 2 + length) > self.buf.len() || (length + 2) > 255 {
            return Err(UsbError::BufferOverflow);
        }

        self.buf[self.pos] = (length + 2) as u8;
        self.buf[self.pos + 1] = descriptor_type;

        let start = self.pos + 2;

        self.buf[start..start + length].copy_from_slice(descriptor);

        self.pos = start + length;

        Ok(())
    }

    pub(crate) fn write_device(&mut self, config: &DeviceConfig) -> Result<()> {
        self.write(
            descriptor_type::DEVICE,
            &[
                0x10,
                0x02,                     // bcdUSB 2.1
                config.device_class,      // bDeviceClass
                config.device_sub_class,  // bDeviceSubClass
                config.device_protocol,   // bDeviceProtocol
                config.max_packet_size_0, // bMaxPacketSize0
                config.vendor_id as u8,
                (config.vendor_id >> 8) as u8, // idVendor
                config.product_id as u8,
                (config.product_id >> 8) as u8, // idProduct
                config.device_release as u8,
                (config.device_release >> 8) as u8,    // bcdDevice
                config.manufacturer.map_or(0, |_| 1),  // iManufacturer
                config.product.map_or(0, |_| 2),       // iProduct
                config.serial_number.map_or(0, |_| 3), // iSerialNumber
                1,                                     // bNumConfigurations
            ],
        )
    }

    pub(crate) fn write_string(&mut self, string: &str) -> Result<()> {
        let mut pos = self.pos;

        if pos + 2 > self.buf.len() {
            return Err(UsbError::BufferOverflow);
        }

        self.buf[pos] = 0; // length placeholder
        self.buf[pos + 1] = descriptor_type::STRING;

        pos += 2;

        for c in string.encode_utf16() {
            if pos >= self.buf.len() {
                return Err(UsbError::BufferOverflow);
            }

            self.buf[pos..pos + 2].copy_from_slice(&c.to_le_bytes());
            pos += 2;
        }

        self.buf[self.pos] = (pos - self.pos) as u8;

        self.pos = pos;

        Ok(())
    }

    /// Finishes the writes and returns the amount of bytes written.
    pub fn finish(self) -> Result<usize> {
        Ok(self.pos())
    }
}

pub(crate) struct ConfigurationDescriptorWriter<'b> {
    writer: DescriptorWriter<'b>,
    enable_iad: bool,
    alt_setting: u8,
    total_length_mark: usize,
    num_interfaces_mark: usize,
    iad_first_interface_mark: Option<usize>,
    iad_interface_count_mark: Option<usize>,
    num_endpoints_mark: Option<usize>,
}

impl ConfigurationDescriptorWriter<'_> {
    pub fn new<'b>(
        mut writer: DescriptorWriter<'b>,
        config: &DeviceConfig,
    ) -> Result<ConfigurationDescriptorWriter<'b>> {
        let total_length_mark = writer.pos() + 2;
        let num_interfaces_mark = writer.pos() + 4;

        writer.write(
            descriptor_type::CONFIGURATION,
            &[
                0,
                0,                   // wTotalLength
                0,                   // bNumInterfaces
                CONFIGURATION_VALUE, // bConfigurationValue
                0,                   // iConfiguration
                0x80 | if config.self_powered { 0x40 } else { 0x00 }
                    | if config.supports_remote_wakeup {
                        0x20
                    } else {
                        0x00
                    }, // bmAttributes
                config.max_power,    // bMaxPower
            ],
        )?;

        Ok(ConfigurationDescriptorWriter {
            writer,
            enable_iad: config.iad_mode == IadMode::Always,
            alt_setting: 0,
            total_length_mark,
            num_interfaces_mark,
            iad_first_interface_mark: None,
            iad_interface_count_mark: None,
            num_endpoints_mark: None,
        })
    }

    fn write_interface(
        &mut self,
        interface_number: u8,
        descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        if let Some(mark) = self.iad_first_interface_mark {
            self.writer.buf()[mark] = interface_number;
            self.iad_first_interface_mark = None;
        }

        if let Some(mark) = self.iad_interface_count_mark {
            self.writer.buf()[mark] += 1;
        }

        self.num_endpoints_mark = Some(self.writer.pos() + 4);

        self.writer.write(
            descriptor_type::INTERFACE,
            &[
                interface_number,       // bInterfaceNumber
                self.alt_setting,       // bAlternateSetting
                0,                      // bNumEndpoints
                descriptor.class,       // bInterfaceClass
                descriptor.sub_class,   // bInterfaceSubClass
                descriptor.protocol,    // bInterfaceProtocol
                descriptor.description, // iInterface
            ],
        )
    }

    fn write_interface_association(&mut self, descriptor: &InterfaceDescriptor) -> Result<()> {
        self.iad_first_interface_mark = Some(self.writer.pos() + 2);
        self.iad_interface_count_mark = Some(self.writer.pos() + 3);

        self.writer.write(
            descriptor_type::INTERFACE_ASSOCIATION,
            &[
                0,                      // bFirstInterface
                0,                      // bInterfaceCount
                descriptor.class,       // bFunctionClass
                descriptor.sub_class,   // bFunctionSubClass
                descriptor.protocol,    // bFunctionProtocol
                descriptor.description, // iFunction
            ],
        )
    }

    fn write_endpoint(
        &mut self,
        addr: EndpointAddress,
        config: &EndpointConfig,
        manual: Option<&[u8]>,
    ) -> Result<()> {
        match self.num_endpoints_mark {
            Some(mark) => self.writer.buf()[mark] += 1,
            None => return Err(UsbError::InvalidState),
        };

        match manual {
            Some(descriptor) => {
                self.writer.write(descriptor_type::ENDPOINT, descriptor)?;
            }
            None => {
                let mps = config.max_packet_size();

                self.writer.write(
                    descriptor_type::ENDPOINT,
                    &[
                        addr.into(),            // bEndpointAddress
                        config.ep_type() as u8, // bmAttributes
                        mps as u8,
                        (mps >> 8) as u8,  // wMaxPacketSize
                        config.interval(), // bInterval
                    ],
                )?;
            }
        }

        Ok(())
    }

    pub fn finish(mut self) -> Result<usize> {
        let position = self.writer.pos() as u16;
        self.writer.buf()[self.total_length_mark..self.total_length_mark + 2]
            .copy_from_slice(&position.to_le_bytes());

        self.writer.finish()
    }
}

impl<U: UsbCore> ConfigVisitor<U> for ConfigurationDescriptorWriter<'_> {
    fn begin_interface(
        &mut self,
        interface: Option<&mut InterfaceHandle>,
        descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        if let Some(interface) = interface {
            self.writer.buf()[self.num_interfaces_mark] += 1;

            self.alt_setting = 0;
            self.num_endpoints_mark = Some(self.writer.pos() + 4);

            self.write_interface(interface.into(), descriptor)?;
        } else if self.enable_iad {
            self.write_interface_association(descriptor)?;
        }

        Ok(())
    }

    fn next_alt_setting(
        &mut self,
        interface_number: u8,
        descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        self.alt_setting += 1;
        self.write_interface(interface_number, descriptor)?;

        Ok(())
    }

    fn end_interface(&mut self, iad: bool) -> () {
        self.num_endpoints_mark = None;

        if iad {
            self.iad_interface_count_mark = None;
        }
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, extra: Option<&[u8]>) -> Result<()> {
        self.write_endpoint(endpoint.address(), &endpoint.config, extra)
    }

    fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>, extra: Option<&[u8]>) -> Result<()> {
        self.write_endpoint(endpoint.address(), &endpoint.config, extra)
    }

    fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        self.writer.write(descriptor_type, descriptor)
    }
}

/// A writer for Binary Object Store (BOS) descriptor.
pub struct BosWriter<'b> {
    writer: DescriptorWriter<'b>,
    num_caps_mark: usize,
}

impl BosWriter<'_> {
    pub(crate) fn new(mut writer: DescriptorWriter) -> Result<BosWriter> {
        let num_caps_mark = writer.pos() + 4;

        writer.write(
            descriptor_type::BOS,
            &[
                0x00, 0x00, // wTotalLength
                0x00, // bNumDeviceCaps
            ],
        )?;

        let mut bos = BosWriter {
            writer,
            num_caps_mark,
        };

        bos.capability(capability_type::USB_2_0_EXTENSION, &[0; 4])?;

        Ok(bos)
    }

    /// Writes capability descriptor to a BOS
    ///
    /// # Arguments
    ///
    /// * `capability_type` - Type of a capability
    /// * `data` - Binary data of the descriptor
    pub fn capability(&mut self, capability_type: u8, data: &[u8]) -> Result<()> {
        self.writer.buf[self.num_caps_mark] += 1;

        let mut start = self.writer.pos;
        let blen = data.len();

        if (start + blen + 3) > self.writer.buf.len() || (blen + 3) > 255 {
            return Err(UsbError::BufferOverflow);
        }

        self.writer.buf[start] = (blen + 3) as u8;
        self.writer.buf[start + 1] = descriptor_type::CAPABILITY;
        self.writer.buf[start + 2] = capability_type;

        start += 3;
        self.writer.buf[start..start + blen].copy_from_slice(data);
        self.writer.pos = start + blen;

        Ok(())
    }

    pub(crate) fn finish(self) -> Result<usize> {
        let position = self.writer.pos() as u16;
        self.writer.buf[2..4].copy_from_slice(&position.to_le_bytes());

        self.writer.finish()
    }
}
