use core::cmp::min;

use crate::bus::{InterfaceNumber, StringIndex, UsbBus};
use crate::device;
use crate::endpoint::{Endpoint, EndpointDirection};
use crate::{Result, UsbError};

/// Standard descriptor types
#[allow(missing_docs)]
pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTERFACE: u8 = 4;
    pub const ENDPOINT: u8 = 5;
    pub const IAD: u8 = 11;
    pub const BOS: u8 = 15;
    pub const CAPABILITY: u8 = 16;
}

/// String descriptor language IDs.
pub mod lang_id;

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
pub struct DescriptorWriter<'a> {
    buf: &'a mut [u8],
    position: usize,
    num_interfaces_mark: Option<usize>,
    num_endpoints_mark: Option<usize>,
    write_iads: bool,
}

impl DescriptorWriter<'_> {
    pub(crate) fn new(buf: &mut [u8]) -> DescriptorWriter<'_> {
        DescriptorWriter {
            buf,
            position: 0,
            num_interfaces_mark: None,
            num_endpoints_mark: None,
            write_iads: false,
        }
    }

    /// Gets the current position in the buffer, i.e. the number of bytes written so far.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Writes an arbitrary (usually class-specific) descriptor.
    pub fn write(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        self.write_with(descriptor_type, |buf| {
            if descriptor.len() > buf.len() {
                return Err(UsbError::BufferOverflow);
            }

            buf[..descriptor.len()].copy_from_slice(descriptor);

            Ok(descriptor.len())
        })
    }

    /// Writes an arbitrary (usually class-specific) descriptor by using a callback function.
    ///
    /// The callback function gets a reference to the remaining buffer space, and it should write
    /// the descriptor into it and return the number of bytes written. If the descriptor doesn't
    /// fit, the function should return `Err(UsbError::BufferOverflow)`. That and any error returned
    /// by it will be propagated up.
    pub fn write_with(
        &mut self,
        descriptor_type: u8,
        f: impl FnOnce(&mut [u8]) -> Result<usize>,
    ) -> Result<()> {
        if self.position + 2 > self.buf.len() {
            return Err(UsbError::BufferOverflow);
        }

        let data_end = min(self.buf.len(), self.position + 256);
        let data_buf = &mut self.buf[self.position + 2..data_end];

        let total_len = f(data_buf)? + 2;

        if self.position + total_len > self.buf.len() {
            return Err(UsbError::BufferOverflow);
        }

        self.buf[self.position] = total_len as u8;
        self.buf[self.position + 1] = descriptor_type;

        self.position += total_len;

        Ok(())
    }

    pub(crate) fn device(&mut self, config: &device::Config) -> Result<()> {
        self.write(
            descriptor_type::DEVICE,
            &[
                (config.usb_rev as u16) as u8,
                (config.usb_rev as u16 >> 8) as u8, // bcdUSB
                config.device_class,                // bDeviceClass
                config.device_sub_class,            // bDeviceSubClass
                config.device_protocol,             // bDeviceProtocol
                config.max_packet_size_0,           // bMaxPacketSize0
                config.vendor_id as u8,
                (config.vendor_id >> 8) as u8, // idVendor
                config.product_id as u8,
                (config.product_id >> 8) as u8, // idProduct
                config.device_release as u8,
                (config.device_release >> 8) as u8, // bcdDevice
                config.string_descriptors.first().map_or(0, |lang| {
                    if lang.manufacturer.is_some() {
                        1
                    } else {
                        0
                    }
                }),
                config.string_descriptors.first().map_or(0, |lang| {
                    if lang.product.is_some() {
                        2
                    } else {
                        0
                    }
                }),
                config.string_descriptors.first().map_or(0, |lang| {
                    if lang.serial.is_some() {
                        3
                    } else {
                        0
                    }
                }),
                1, // bNumConfigurations
            ],
        )
    }

    pub(crate) fn configuration(&mut self, config: &device::Config) -> Result<()> {
        self.num_interfaces_mark = Some(self.position + 4);

        self.write_iads = config.composite_with_iads;

        self.write(
            descriptor_type::CONFIGURATION,
            &[
                0,
                0,                           // wTotalLength
                0,                           // bNumInterfaces
                device::CONFIGURATION_VALUE, // bConfigurationValue
                0,                           // iConfiguration
                0x80 | if config.self_powered { 0x40 } else { 0x00 }
                    | if config.supports_remote_wakeup {
                        0x20
                    } else {
                        0x00
                    }, // bmAttributes
                config.max_power,            // bMaxPower
            ],
        )
    }

    pub(crate) fn end_class(&mut self) {
        self.num_endpoints_mark = None;
    }

    pub(crate) fn end_configuration(&mut self) {
        let position = self.position as u16;
        self.buf[2..4].copy_from_slice(&position.to_le_bytes());
    }

    /// Writes a interface association descriptor. Call from `UsbClass::get_configuration_descriptors`
    /// before writing the USB class or function's interface descriptors if your class has more than
    /// one interface and wants to play nicely with composite devices on Windows. If the USB device
    /// hosting the class was not configured as composite with IADs enabled, calling this function
    /// does nothing, so it is safe to call from libraries.
    ///
    /// # Arguments
    ///
    /// * `first_interface` - Number of the function's first interface, previously allocated with
    ///   [`UsbBusAllocator::interface`](crate::bus::UsbBusAllocator::interface).
    /// * `interface_count` - Number of interfaces in the function.
    /// * `function_class` - Class code assigned by USB.org. Use `0xff` for vendor-specific devices
    ///   that do not conform to any class.
    /// * `function_sub_class` - Sub-class code. Depends on class.
    /// * `function_protocol` - Protocol code. Depends on class and sub-class.
    /// * `function_string` - Index of string descriptor describing this function
    pub fn iad(
        &mut self,
        first_interface: InterfaceNumber,
        interface_count: u8,
        function_class: u8,
        function_sub_class: u8,
        function_protocol: u8,
        function_string: Option<StringIndex>,
    ) -> Result<()> {
        if !self.write_iads {
            return Ok(());
        }

        let str_index = function_string.map_or(0, Into::into);

        self.write(
            descriptor_type::IAD,
            &[
                first_interface.into(), // bFirstInterface
                interface_count,        // bInterfaceCount
                function_class,
                function_sub_class,
                function_protocol,
                str_index,
            ],
        )?;

        Ok(())
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
    pub fn interface(
        &mut self,
        number: InterfaceNumber,
        interface_class: u8,
        interface_sub_class: u8,
        interface_protocol: u8,
    ) -> Result<()> {
        self.interface_alt(
            number,
            device::DEFAULT_ALTERNATE_SETTING,
            interface_class,
            interface_sub_class,
            interface_protocol,
            None,
        )
    }

    /// Writes a interface descriptor with a specific alternate setting and
    /// interface string identifier.
    ///
    /// # Arguments
    ///
    /// * `number` - Interface number previously allocated with
    ///   [`UsbBusAllocator::interface`](crate::bus::UsbBusAllocator::interface).
    /// * `alternate_setting` - Number of the alternate setting
    /// * `interface_class` - Class code assigned by USB.org. Use `0xff` for vendor-specific devices
    ///   that do not conform to any class.
    /// * `interface_sub_class` - Sub-class code. Depends on class.
    /// * `interface_protocol` - Protocol code. Depends on class and sub-class.
    /// * `interface_string` - Index of string descriptor describing this interface

    pub fn interface_alt(
        &mut self,
        number: InterfaceNumber,
        alternate_setting: u8,
        interface_class: u8,
        interface_sub_class: u8,
        interface_protocol: u8,
        interface_string: Option<StringIndex>,
    ) -> Result<()> {
        if alternate_setting == device::DEFAULT_ALTERNATE_SETTING {
            match self.num_interfaces_mark {
                Some(mark) => self.buf[mark] += 1,
                None => return Err(UsbError::InvalidState),
            };
        }

        let str_index = interface_string.map_or(0, Into::into);

        self.num_endpoints_mark = Some(self.position + 4);

        self.write(
            descriptor_type::INTERFACE,
            &[
                number.into(),       // bInterfaceNumber
                alternate_setting,   // bAlternateSetting
                0,                   // bNumEndpoints
                interface_class,     // bInterfaceClass
                interface_sub_class, // bInterfaceSubClass
                interface_protocol,  // bInterfaceProtocol
                str_index,           // iInterface
            ],
        )?;

        Ok(())
    }

    /// Writes an endpoint descriptor.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint previously allocated with
    ///   [`UsbBusAllocator`](crate::bus::UsbBusAllocator).
    pub fn endpoint<B: UsbBus, D: EndpointDirection>(
        &mut self,
        endpoint: &Endpoint<'_, B, D>,
    ) -> Result<()> {
        self.endpoint_ex(endpoint, |_| Ok(0))
    }

    /// Writes an endpoint descriptor with extra trailing data.
    ///
    /// This is rarely needed and shouldn't be used except for compatibility with standard USB
    /// classes that require it. Extra data is normally written in a separate class specific
    /// descriptor.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint previously allocated with
    ///   [`UsbBusAllocator`](crate::bus::UsbBusAllocator).
    /// * `f` - Callback for the extra data. See `write_with` for more information.
    pub fn endpoint_ex<B: UsbBus, D: EndpointDirection>(
        &mut self,
        endpoint: &Endpoint<'_, B, D>,
        f: impl FnOnce(&mut [u8]) -> Result<usize>,
    ) -> Result<()> {
        match self.num_endpoints_mark {
            Some(mark) => self.buf[mark] += 1,
            None => return Err(UsbError::InvalidState),
        };

        self.write_with(descriptor_type::ENDPOINT, |buf| {
            if buf.len() < 5 {
                return Err(UsbError::BufferOverflow);
            }

            let mps = endpoint.max_packet_size();

            buf[0] = endpoint.address().into();
            buf[1] = endpoint.ep_type().to_bm_attributes();
            buf[2] = mps as u8;
            buf[3] = (mps >> 8) as u8;
            buf[4] = endpoint.interval();

            Ok(f(&mut buf[5..])? + 5)
        })
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

/// A writer for Binary Object Store descriptor.
pub struct BosWriter<'w, 'a: 'w> {
    writer: &'w mut DescriptorWriter<'a>,
    num_caps_mark: Option<usize>,
}

impl<'w, 'a: 'w> BosWriter<'w, 'a> {
    pub(crate) fn new(writer: &'w mut DescriptorWriter<'a>) -> Self {
        Self {
            writer,
            num_caps_mark: None,
        }
    }

    pub(crate) fn bos(&mut self) -> Result<()> {
        self.num_caps_mark = Some(self.writer.position + 4);
        self.writer.write(
            descriptor_type::BOS,
            &[
                0x00, 0x00, // wTotalLength
                0x00, // bNumDeviceCaps
            ],
        )?;

        self.capability(capability_type::USB_2_0_EXTENSION, &[0; 4])?;

        Ok(())
    }

    /// Writes capability descriptor to a BOS
    ///
    /// # Arguments
    ///
    /// * `capability_type` - Type of a capability
    /// * `data` - Binary data of the descriptor
    pub fn capability(&mut self, capability_type: u8, data: &[u8]) -> Result<()> {
        match self.num_caps_mark {
            Some(mark) => self.writer.buf[mark] += 1,
            None => return Err(UsbError::InvalidState),
        }

        let mut start = self.writer.position;
        let blen = data.len();

        if (start + blen + 3) > self.writer.buf.len() || (blen + 3) > 255 {
            return Err(UsbError::BufferOverflow);
        }

        self.writer.buf[start] = (blen + 3) as u8;
        self.writer.buf[start + 1] = descriptor_type::CAPABILITY;
        self.writer.buf[start + 2] = capability_type;

        start += 3;
        self.writer.buf[start..start + blen].copy_from_slice(data);
        self.writer.position = start + blen;

        Ok(())
    }

    pub(crate) fn end_bos(&mut self) {
        self.num_caps_mark = None;
        let position = self.writer.position as u16;
        self.writer.buf[2..4].copy_from_slice(&position.to_le_bytes());
    }
}
