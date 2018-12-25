#![allow(missing_docs)]

use core::cmp;
use crate::Result;
use crate::class_prelude::*;
use crate::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};
use crate::descriptor;

/// Test USB class for testing USB driver implementations. Supports various endpoint types and
/// requests for testing USB peripheral drivers on actual hardware.
pub struct TestClass<'a, B: UsbBus> {
    custom_string: StringIndex,
    iface: InterfaceNumber,
    ep_bulk_in: EndpointIn<'a, B>,
    ep_bulk_out: EndpointOut<'a, B>,
    ep_interrupt_in: EndpointIn<'a, B>,
    ep_interrupt_out: EndpointOut<'a, B>,
    control_buf: [u8; 256],
    bulk_buf: [u8; 256],
    interrupt_buf: [u8; 256],
    len: usize,
    i: usize,
    bench: bool,
    expect_bulk_in_complete: bool,
    expect_bulk_out: bool,
    expect_interrupt_in_complete: bool,
    expect_interrupt_out: bool,
}

pub const VID: u16 = 0x16c0;
pub const PID: u16 = 0x05dc;
pub const MANUFACTURER: &'static str = "TestClass Manufacturer";
pub const PRODUCT: &'static str = "virkkunen.net usb-device TestClass";
pub const SERIAL_NUMBER: &'static str = "TestClass Serial";
pub const CUSTOM_STRING: &'static str = "TestClass Custom String";

pub const REQ_STORE_REQUEST: u8 = 1;
pub const REQ_READ_BUFFER: u8 = 2;
pub const REQ_WRITE_BUFFER: u8 = 3;
pub const REQ_SET_BENCH_ENABLED: u8 = 4;
pub const REQ_UNKNOWN: u8 = 42;

impl<B: UsbBus> TestClass<'_, B> {
    /// Creates a new TestClass.
    pub fn new(alloc: &UsbBusAllocator<B>) -> TestClass<'_, B> {
        TestClass {
            custom_string: alloc.string(),
            iface: alloc.interface(),
            ep_bulk_in: alloc.bulk(64),
            ep_bulk_out: alloc.bulk(64),
            ep_interrupt_in: alloc.interrupt(31, 1),
            ep_interrupt_out: alloc.interrupt(31, 1),
            control_buf: [0; 256],
            bulk_buf: [0; 256],
            interrupt_buf: [0; 256],
            len: 0,
            i: 0,
            bench: false,
            expect_bulk_in_complete: false,
            expect_bulk_out: false,
            expect_interrupt_in_complete: false,
            expect_interrupt_out: false,
        }
    }

    /// Convenience method to create a UsbDevice that is configured correctly for TestClass.
    pub fn make_device<'a, 'b>(&'a self, usb_bus: &'b UsbBusAllocator<B>) -> UsbDevice<'b, B> {
        UsbDeviceBuilder::new(&usb_bus, UsbVidPid(VID, PID))
            .manufacturer(MANUFACTURER)
            .product(PRODUCT)
            .serial_number(SERIAL_NUMBER)
            .build()
    }

    /// Must be called after polling the UsbDevice.
    pub fn poll(&mut self) {
        if self.bench {
            match self.ep_bulk_out.read(&mut self.bulk_buf) {
                Ok(_) | Err(UsbError::WouldBlock) => { },
                Err(err) => panic!("bulk bench read {:?}", err),
            };

            match self.ep_bulk_in.write(&self.bulk_buf[0..self.ep_bulk_in.max_packet_size() as usize]) {
                Ok(_) | Err(UsbError::WouldBlock) => { },
                Err(err) => panic!("bulk bench write {:?}", err),
            };

            return;
        }

        let temp_i = self.i;
        match self.ep_bulk_out.read(&mut self.bulk_buf[temp_i..]) {
            Ok(count) => {
                if self.expect_bulk_out {
                    self.expect_bulk_out = false;
                } else {
                    panic!("unexpectedly read data from bulk out endpoint");
                }

                self.i += count;

                if count < self.ep_bulk_out.max_packet_size() as usize {
                    self.len = self.i;
                    self.i = 0;

                    self.write_bulk_in(count == 0);
                }
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("bulk read {:?}", err),
        };

        match self.ep_interrupt_out.read(&mut self.interrupt_buf) {
            Ok(count) => {
                if self.expect_interrupt_out {
                    self.expect_interrupt_out = false;
                } else {
                    panic!("unexpectedly read data from interrupt out endpoint");
                }

                self.ep_interrupt_in.write(&self.interrupt_buf[0..count])
                    .expect("interrupt write");

                self.expect_interrupt_in_complete = true;
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("interrupt read {:?}", err),
        };
    }

    fn write_bulk_in(&mut self, write_empty: bool) {
        let to_write = cmp::min(self.len - self.i, self.ep_bulk_in.max_packet_size() as usize);

        if to_write == 0 && !write_empty {
            self.len = 0;
            self.i = 0;

            return;
        }

        match self.ep_bulk_in.write(&self.bulk_buf[self.i..self.i+to_write]) {
            Ok(count) => {
                assert_eq!(count, to_write);
                self.expect_bulk_in_complete = true;
                self.i += count;
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("bulk write {:?}", err),
        };
    }
}

impl<B: UsbBus> UsbClass<B> for TestClass<'_, B> {
    fn reset(&mut self) {
        self.len = 0;
        self.i = 0;
        self.bench = false;
        self.expect_bulk_in_complete = false;
        self.expect_bulk_out = false;
        self.expect_interrupt_in_complete = false;
        self.expect_interrupt_out = false;
    }

    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface(self.iface, 0xff, 0x00, 0x00)?;
        writer.endpoint(&self.ep_bulk_in)?;
        writer.endpoint(&self.ep_bulk_out)?;
        writer.endpoint(&self.ep_interrupt_in)?;
        writer.endpoint(&self.ep_interrupt_out)?;

        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        if index == self.custom_string && lang_id == descriptor::lang_id::ENGLISH_US {
            Some(CUSTOM_STRING)
        } else {
            None
        }
    }

    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        if self.bench {
            return;
        }

        if addr == self.ep_bulk_in.address() {
            if self.expect_bulk_in_complete {
                self.expect_bulk_in_complete = false;

                self.write_bulk_in(false);
            } else {
                panic!("unexpected endpoint_in_complete");
            }
        } else if addr == self.ep_interrupt_in.address() {
            if self.expect_interrupt_in_complete {
                self.expect_interrupt_in_complete = false;
            } else {
                panic!("unexpected endpoint_in_complete");
            }
        }
    }

    fn endpoint_out(&mut self, addr: EndpointAddress) {
        if addr == self.ep_bulk_out.address() {
            self.expect_bulk_out = true;
        } else if addr == self.ep_interrupt_out.address() {
            self.expect_interrupt_out = true;
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        match req.request {
            REQ_READ_BUFFER if req.length as usize <= self.control_buf.len()
                => xfer.accept_with(&self.control_buf[0..req.length as usize]).unwrap(),
            _ => xfer.reject().unwrap(),
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        match req.request {
            REQ_STORE_REQUEST => {
                self.control_buf[0] = (req.direction as u8) | (req.request_type as u8) << 5 | (req.recipient as u8);
                self.control_buf[1] = req.request;
                self.control_buf[2..4].copy_from_slice(&req.value.to_le_bytes());
                self.control_buf[4..6].copy_from_slice(&req.index.to_le_bytes());
                self.control_buf[6..8].copy_from_slice(&req.length.to_le_bytes());

                xfer.accept().unwrap();
            },
            REQ_WRITE_BUFFER if xfer.data().len() as usize <= self.control_buf.len() => {
                assert_eq!(xfer.data().len(), req.length as usize, "xfer data len == req.length");

                self.control_buf[0..xfer.data().len()].copy_from_slice(xfer.data());

                xfer.accept().unwrap();
            },
            REQ_SET_BENCH_ENABLED => {
                self.bench = req.value != 0;

                xfer.accept().unwrap();
            },
            _ => xfer.reject().unwrap()
        }
    }
}

