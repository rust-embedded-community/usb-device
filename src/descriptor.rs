use core::mem;
use core::slice;
use ::{Result, UsbError};
use bus::Endpoint;

pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTERFACE: u8 = 4;
    pub const ENDPOINT: u8 = 5;
}

pub mod lang_id {
    pub const ENGLISH_US: u16 = 0x0409;
}

pub struct DescriptorWriter<'a> {
    buf: &'a mut [u8],
    i: usize,
    next_interface_number: u8,
}

impl<'a> DescriptorWriter<'a> {
    pub(crate) fn new(buf: &'a mut [u8]) -> Self {
        DescriptorWriter {
            buf,
            i: 0,
            next_interface_number: 0,
        }
    }

    pub(crate) fn num_interfaces(&self) -> u8 { self.next_interface_number }

    pub(crate) fn count(&self) -> usize { self.i }

    fn write_header(&mut self, length: usize, descriptor_type: u8) -> Result<()> {
        if self.i + length + 2 as usize > self.buf.len() {
            return Err(UsbError::BufferOverflow);
        }

        self.buf[self.i] = (length + 2) as u8;
        self.buf[self.i + 1] = descriptor_type;
        self.i += 2;

        Ok(())
    }

    pub(crate) fn insert(&mut self, index: usize, data: &[u8]) {
        self.buf[index..index+data.len()].copy_from_slice(data);
    }

    pub fn write(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        let length = descriptor.len();

        self.write_header(length, descriptor_type)?;

        self.buf[self.i..self.i+length].copy_from_slice(descriptor);
        self.i += length;

        Ok(())
    }

    pub(crate) fn write_string(&mut self, string: &str) -> Result<()> {
        let mut buf: [u16; 64] = unsafe { mem::uninitialized() };
        let mut i = 0;

        for c in string.chars() {
            let c = c as u32;

            if c < 0x10000 {
                buf[i] = (c as u16).to_le();

                i += 1;
            } else {
                let c = c - 0x10000;

                buf[i] = (((c >> 10) + 0xd800) as u16).to_le();
                buf[i + 1] = (((c & 0x003f) + 0xdc00) as u16).to_le();
                i += 2;
            }
        }

        let length = i * 2;

        self.write_header(length, descriptor_type::STRING)?;

        self.buf[self.i..self.i+length].copy_from_slice(
            unsafe { slice::from_raw_parts(&buf[0] as *const u16 as *const u8, length) });
        self.i += length;

        Ok(())
    }

    pub fn interface(&mut self, num_endpoints: u8, interface_class: u8, interface_sub_class: u8, interface_protocol: u8) -> Result<u8> {
        let number = self.next_interface_number;
        self.next_interface_number += 1;

        self.write(
            descriptor_type::INTERFACE,
            &[
                number, // bInterfaceNumber
                0, // bAlternateSetting (how to even handle these...)
                num_endpoints, // bNumEndpoints
                interface_class, // bInterfaceClass
                interface_sub_class, // bInterfaceSubClass
                interface_protocol, // bInterfaceProtocol
                0, // iInterface
            ])?;

        Ok(number)
    }

    pub fn endpoint<T: Endpoint>(&mut self, endpoint: &T) -> Result<()> {
        let mps = endpoint.max_packet_size();

        self.write(
            descriptor_type::ENDPOINT,
            &[
                endpoint.address(), // bEndpointAddress
                endpoint.ep_type() as u8, // bmAttributes
                mps as u8, (mps >> 8) as u8, // wMaxPacketSize
                endpoint.interval(), // bInterval
            ])?;

        Ok(())
    }
}

/*pub struct ClassDescriptorWriter<'a> {
    writer: &'a mut DescriptorWriter<'a>,
}

impl<'a> ClassDescriptorWriter<'a> {
    pub fn new(writer: &'a mut DescriptorWriter<'a>) -> ClassDescriptorWriter {
        ClassDescriptorWriter {
            writer,
            next_interface_number: 0,
        }
    }
}*/

/*pub struct ClassDescriptorWriter<'a> {
    writer: &'a mut DescriptorWriter<'a>,
}

impl<'a> ClassDescriptorWriter<'a> {
    pub fn interface<'b: 'a>(&'b mut self, interface_class: u8, interface_sub_class: u8, interface_protocol: u8)
        -> InterfaceDescriptorWriter<'b>
    {
        let number = self.writer.next_interface_number;
        self.writer.next_interface_number += 1;

        self.writer.write(
            descriptor_type::INTERFACE,
            &[
                number, // bInterfaceNumber
                0, // bAlternateSettings (how to even handle these...)
                0, // bNumEndpoints (placeholder)
                interface_class, // bInterfaceClass
                interface_sub_class, // bInterfaceSubClass
                interface_protocol, // bInterfaceProtocol
                0, // iInterface
            ]
        );

        InterfaceDescriptorWriter {
            writer: self.writer,
            number
        }
    }
}

pub struct InterfaceDescriptorWriter<'a> {
    writer: &'a mut DescriptorWriter<'a>,
    number: u8,
}

impl<'a> InterfaceDescriptorWriter<'a> {
    pub fn endpoint<T: Endpoint>(&mut self, endpoint: &T) {
        self.writer.write(
            descriptor_type::ENDPOINT,
            &[]);
    }
}*/