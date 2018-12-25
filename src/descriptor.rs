use crate::{Result, UsbError};
use crate::bus::{UsbBus, InterfaceNumber};
use crate::device;
use crate::endpoint::{Endpoint, EndpointDirection};

/// Standard descriptor types
#[allow(missing_docs)]
pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTERFACE: u8 = 4;
    pub const ENDPOINT: u8 = 5;
}

/// String descriptor language IDs.
pub mod lang_id {
    /// English (US)
    ///
    /// Recommended for use as the first language ID for compatibility.
    pub const ENGLISH_US: u16 = 0x0409;
}

/// A writer for USB descriptors.
pub struct DescriptorWriter<'a> {
    buf: &'a mut [u8],
    position: usize,
    num_interfaces_mark: Option<usize>,
    num_endpoints_mark: Option<usize>,
}

impl DescriptorWriter<'_> {
    pub(crate) fn new(buf: &mut [u8]) -> DescriptorWriter<'_> {
        DescriptorWriter {
            buf,
            position: 0,
            num_interfaces_mark: None,
            num_endpoints_mark: None,
        }
    }

    /// Gets the current position in the buffer, i.e. the number of bytes written so far.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Writes an arbitrary (usually class-specific) descriptor.
    pub fn write(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        let length = descriptor.len();

        if self.position + 2 + length > self.buf.len() {
            return Err(UsbError::BufferOverflow);
        }

        self.buf[self.position] = (length + 2) as u8;
        self.buf[self.position + 1] = descriptor_type;

        let start = self.position + 2;

        self.buf[start..start + length].copy_from_slice(descriptor);

        self.position = start + length;

        Ok(())
    }

    pub(crate) fn device(&mut self, config: &device::Config) -> Result<()> {
        self.write(
            descriptor_type::DEVICE,
            &[
                0x00, 0x02, // bcdUSB
                config.device_class, // bDeviceClass
                config.device_sub_class, // bDeviceSubClass
                config.device_protocol, // bDeviceProtocol
                config.max_packet_size_0, // bMaxPacketSize0
                config.vendor_id as u8, (config.vendor_id >> 8) as u8, // idVendor
                config.product_id as u8, (config.product_id >> 8) as u8, // idProduct
                config.device_release as u8, (config.device_release >> 8) as u8, // bcdDevice
                config.manufacturer.map_or(0, |_| 1), // iManufacturer
                config.product.map_or(0, |_| 2), // iProduct
                config.serial_number.map_or(0, |_| 3), // iSerialNumber
                1, // bNumConfigurations
            ])
    }

    pub(crate) fn configuration(&mut self, config: &device::Config) -> Result<()> {
        self.num_interfaces_mark = Some(self.position + 4);

        self.write(
            descriptor_type::CONFIGURATION,
            &[
                0, 0, // wTotalLength
                0, // bNumInterfaces
                device::CONFIGURATION_VALUE, // bConfigurationValue
                0, // iConfiguration
                0x80
                    | if config.self_powered { 0x40 } else { 0x00 }
                    | if config.supports_remote_wakeup { 0x20 } else { 0x00 }, // bmAttributes
                config.max_power // bMaxPower
            ])
    }

    pub(crate) fn end_class(&mut self) {
        self.num_endpoints_mark = None;
    }

    pub(crate) fn end_configuration(&mut self) {
        let position = self.position as u16;
        self.buf[2..4].copy_from_slice(&position.to_le_bytes());
    }

    /// Writes a interface descriptor.
    ///
    /// # Arguments
    ///
    /// * `number` - Interface number previously allocated with
    ///   [`UsbBusAllocator::interface`](crate::bus::UsbBusAllocator::interface).
    /// * `interface_class` - Class code assigned by USB.org. Use `0xff` for vendor-specific devices
    ///   that do not conform to any class.
    /// * `interface_sub_class` - Sub-class code. Depends on class.
    /// * `interface_protocol` - Protocol code. Depends on class and sub-class.
    pub fn interface(&mut self, number: InterfaceNumber,
        interface_class: u8, interface_sub_class: u8, interface_protocol: u8) -> Result<()>
    {
        self.buf[self.num_interfaces_mark.unwrap()] += 1;

        self.num_endpoints_mark = Some(self.position + 4);

        self.write(
            descriptor_type::INTERFACE,
            &[
                number.into(), // bInterfaceNumber
                device::DEFAULT_ALTERNATE_SETTING, // bAlternateSetting (how to even handle these...)
                0, // bNumEndpoints
                interface_class, // bInterfaceClass
                interface_sub_class, // bInterfaceSubClass
                interface_protocol, // bInterfaceProtocol
                0, // iInterface
            ])?;

        Ok(())
    }

    /// Writes an endpoint descriptor.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint previously allocated with
    ///   [`UsbBusAllocator`](crate::bus::UsbBusAllocator).
    pub fn endpoint<'e, B: UsbBus, D: EndpointDirection>(&mut self, endpoint: &Endpoint<'e, B, D>)
        -> Result<()>
    {
        self.buf[self.num_endpoints_mark.expect("missing interface descriptor")] += 1;

        let mps = endpoint.max_packet_size();

        self.write(
            descriptor_type::ENDPOINT,
            &[
                endpoint.address().into(), // bEndpointAddress
                endpoint.ep_type() as u8, // bmAttributes
                mps as u8, (mps >> 8) as u8, // wMaxPacketSize
                endpoint.interval(), // bInterval
            ])?;

        Ok(())
    }

    /// Writes a string descriptor.
    pub(crate) fn string(&mut self, string: &str) -> Result<()> {
        let mut pos = self.position;

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

        self.buf[self.position] = (pos - self.position) as u8;

        self.position = pos;

        Ok(())
    }
}
