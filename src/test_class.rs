#![allow(missing_docs)]

use crate::class_prelude::*;
use crate::descriptor;
use crate::device::{UsbDevice, UsbDeviceBuilder, UsbVidPid};
use crate::Result;
use core::cmp;

#[cfg(feature = "test-class-highspeed")]
mod sizes {
    pub const BUFFER: usize = 1024;
    pub const BULK_ENDPOINT: u16 = 512;
    pub const INTERRUPT_ENDPOINT: u16 = 1024;
}

#[cfg(not(feature = "test-class-highspeed"))]
mod sizes {
    pub const BUFFER: usize = 256;
    pub const BULK_ENDPOINT: u16 = 64;
    pub const INTERRUPT_ENDPOINT: u16 = 31;
}

/// Test USB class for testing USB driver implementations. Supports various endpoint types and
/// requests for testing USB peripheral drivers on actual hardware.
pub struct TestClass<U: UsbCore> {
    custom_string: StringHandle,
    iface: InterfaceHandle,
    ep_bulk_in: EndpointIn<U>,
    ep_bulk_out: EndpointOut<U>,
    ep_interrupt_in: EndpointIn<U>,
    ep_interrupt_out: EndpointOut<U>,
    control_buf: [u8; sizes::BUFFER],
    bulk_buf: [u8; sizes::BUFFER],
    interrupt_buf: [u8; sizes::BUFFER],
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
pub const REQ_READ_LONG_DATA: u8 = 5;
pub const REQ_UNKNOWN: u8 = 42;

pub const LONG_DATA: &'static [u8] = &[0x17; 257];

impl<U: UsbCore> TestClass<U> {
    /// Creates a new TestClass.
    pub fn new() -> TestClass<U> {
        TestClass {
            custom_string: StringHandle::new(),
            iface: InterfaceHandle::new(),
            ep_bulk_in: EndpointConfig::bulk(sizes::BULK_ENDPOINT).into(),
            ep_bulk_out: EndpointConfig::bulk(sizes::BULK_ENDPOINT).into(),
            ep_interrupt_in: EndpointConfig::interrupt(sizes::INTERRUPT_ENDPOINT, 1).into(),
            ep_interrupt_out: EndpointConfig::interrupt(sizes::INTERRUPT_ENDPOINT, 1).into(),
            control_buf: [0; sizes::BUFFER],
            bulk_buf: [0; sizes::BUFFER],
            interrupt_buf: [0; sizes::BUFFER],
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
    pub fn make_device(&mut self, usb: U) -> UsbDevice<U> {
        UsbDeviceBuilder::new(usb, UsbVidPid(VID, PID))
            .manufacturer(MANUFACTURER)
            .product(PRODUCT)
            .serial_number(SERIAL_NUMBER)
            .build(&mut [self])
    }

    /// Must be called after polling the UsbDevice.
    pub fn poll(&mut self, state: UsbDeviceState) {
        if state != UsbDeviceState::Configured {
            return;
        }

        if self.bench {
            match self.ep_bulk_out.read_packet(&mut self.bulk_buf) {
                Ok(_) | Err(UsbError::WouldBlock) => {}
                Err(err) => panic!("bulk bench read {:?}", err),
            };

            match self
                .ep_bulk_in
                .write_packet(&self.bulk_buf[0..self.ep_bulk_in.max_packet_size() as usize])
            {
                Ok(_) | Err(UsbError::WouldBlock) => {}
                Err(err) => panic!("bulk bench write {:?}", err),
            };

            return;
        }

        let temp_i = self.i;
        match self.ep_bulk_out.read_packet(&mut self.bulk_buf[temp_i..]) {
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
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("bulk read {:?}", err),
        };

        match self.ep_interrupt_out.read_packet(&mut self.interrupt_buf) {
            Ok(count) => {
                if self.expect_interrupt_out {
                    self.expect_interrupt_out = false;
                } else {
                    panic!("unexpectedly read data from interrupt out endpoint");
                }

                self.ep_interrupt_in
                    .write_packet(&self.interrupt_buf[0..count])
                    .expect("interrupt write");

                self.expect_interrupt_in_complete = true;
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("interrupt read {:?}", err),
        };
    }

    fn write_bulk_in(&mut self, write_empty: bool) {
        let count = cmp::min(
            self.len - self.i,
            self.ep_bulk_in.max_packet_size() as usize,
        );

        if count == 0 && !write_empty {
            self.len = 0;
            self.i = 0;

            return;
        }

        match self
            .ep_bulk_in
            .write_packet(&self.bulk_buf[self.i..self.i + count])
        {
            Ok(()) => {
                self.expect_bulk_in_complete = true;
                self.i += count;
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("bulk write {:?}", err),
        };
    }
}

impl<U: UsbCore> UsbClass<U> for TestClass<U> {
    fn configure(&mut self, mut config: Config<U>) -> Result<()> {
        config
            .interface(
                &mut self.iface,
                InterfaceDescriptor {
                    class: 0xff,
                    ..Default::default()
                },
            )?
            .endpoint_in(&mut self.ep_bulk_in)?
            .endpoint_out(&mut self.ep_bulk_out)?
            .endpoint_in(&mut self.ep_interrupt_in)?
            .endpoint_out(&mut self.ep_interrupt_out)?;

        Ok(())
    }

    fn reset(&mut self) {
        self.len = 0;
        self.i = 0;
        self.bench = false;
        self.expect_bulk_in_complete = false;
        self.expect_bulk_out = false;
        self.expect_interrupt_in_complete = false;
        self.expect_interrupt_out = false;
    }

    fn get_string(&self, index: StringHandle, lang_id: u16) -> Option<&str> {
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

    fn control_in(&mut self, xfer: ControlIn<U>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        match req.request {
            REQ_READ_BUFFER if req.length as usize <= self.control_buf.len() => xfer
                .accept_with(&self.control_buf[0..req.length as usize])
                .expect("control_in REQ_READ_BUFFER failed"),
            REQ_READ_LONG_DATA => xfer
                .accept_with_static(LONG_DATA)
                .expect("control_in REQ_READ_LONG_DATA failed"),
            _ => xfer.reject().expect("control_in reject failed"),
        }
    }

    fn control_out(&mut self, xfer: ControlOut<U>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        match req.request {
            REQ_STORE_REQUEST => {
                self.control_buf[0] =
                    (req.direction as u8) | (req.request_type as u8) << 5 | (req.recipient as u8);
                self.control_buf[1] = req.request;
                self.control_buf[2..4].copy_from_slice(&req.value.to_le_bytes());
                self.control_buf[4..6].copy_from_slice(&req.index.to_le_bytes());
                self.control_buf[6..8].copy_from_slice(&req.length.to_le_bytes());

                xfer.accept().expect("control_out REQ_STORE_REQUEST failed");
            }
            REQ_WRITE_BUFFER if xfer.data().len() as usize <= self.control_buf.len() => {
                assert_eq!(
                    xfer.data().len(),
                    req.length as usize,
                    "xfer data len == req.length"
                );

                self.control_buf[0..xfer.data().len()].copy_from_slice(xfer.data());

                xfer.accept().expect("control_out REQ_WRITE_BUFFER failed");
            }
            REQ_SET_BENCH_ENABLED => {
                self.bench = req.value != 0;

                xfer.accept()
                    .expect("control_out REQ_SET_BENCH_ENABLED failed");
            }
            _ => xfer.reject().expect("control_out reject failed"),
        }
    }
}
