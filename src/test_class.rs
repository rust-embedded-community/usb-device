#![allow(missing_docs)]

use crate::class_prelude::*;
use crate::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};
use crate::Result;
use core::cell::UnsafeCell;
use core::cmp;

#[cfg(feature = "test-class-high-speed")]
mod sizes {
    pub const BUFFER: usize = 2048;
    pub const CONTROL_ENDPOINT: u8 = 64;
    pub const BULK_ENDPOINT: u16 = 512;
    pub const INTERRUPT_ENDPOINT: u16 = 1024;
}

#[cfg(not(feature = "test-class-high-speed"))]
mod sizes {
    pub const BUFFER: usize = 256;
    pub const CONTROL_ENDPOINT: u8 = 8;
    pub const BULK_ENDPOINT: u16 = 64;
    pub const INTERRUPT_ENDPOINT: u16 = 31;
}

static mut CONTROL_BUFFER: UnsafeCell<[u8; 256]> = UnsafeCell::new([0; 256]);

/// Test USB class for testing USB driver implementations. Supports various endpoint types and
/// requests for testing USB peripheral drivers on actual hardware.
pub struct TestClass<'a, B: UsbBus> {
    custom_string: StringIndex,
    interface_string: StringIndex,
    iface: InterfaceNumber,
    ep_bulk_in: EndpointIn<'a, B>,
    ep_bulk_out: EndpointOut<'a, B>,
    ep_interrupt_in: EndpointIn<'a, B>,
    ep_interrupt_out: EndpointOut<'a, B>,
    ep_iso_in: EndpointIn<'a, B>,
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
pub const MANUFACTURER: &str = "TestClass Manufacturer";
pub const PRODUCT: &str = "virkkunen.net usb-device TestClass";
pub const SERIAL_NUMBER: &str = "TestClass Serial";
pub const CUSTOM_STRING: &str = "TestClass Custom String";
pub const INTERFACE_STRING: &str = "TestClass Interface";

pub const REQ_STORE_REQUEST: u8 = 1;
pub const REQ_READ_BUFFER: u8 = 2;
pub const REQ_WRITE_BUFFER: u8 = 3;
pub const REQ_SET_BENCH_ENABLED: u8 = 4;
pub const REQ_READ_LONG_DATA: u8 = 5;
pub const REQ_UNKNOWN: u8 = 42;

pub const LONG_DATA: &[u8] = &[0x17; 257];

impl<B: UsbBus> TestClass<'_, B> {
    /// Creates a new TestClass.
    pub fn new(alloc: &UsbBusAllocator<B>) -> TestClass<'_, B> {
        TestClass {
            custom_string: alloc.string(),
            interface_string: alloc.string(),
            iface: alloc.interface(),
            ep_bulk_in: alloc.bulk(sizes::BULK_ENDPOINT),
            ep_bulk_out: alloc.bulk(sizes::BULK_ENDPOINT),
            ep_interrupt_in: alloc.interrupt(sizes::INTERRUPT_ENDPOINT, 1),
            ep_interrupt_out: alloc.interrupt(sizes::INTERRUPT_ENDPOINT, 1),
            ep_iso_in: alloc.isochronous(
                IsochronousSynchronizationType::Asynchronous,
                IsochronousUsageType::ImplicitFeedbackData,
                500, // These last two args are arbitrary in this usage, they
                1,   // let the host know how much bandwidth to reserve.
            ),
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
    pub fn make_device<'a>(&self, usb_bus: &'a UsbBusAllocator<B>) -> UsbDevice<'a, B> {
        self.make_device_builder(usb_bus).build().unwrap()
    }

    /// Convenience method to create a UsbDeviceBuilder that is configured correctly for TestClass.
    ///
    /// The methods sets
    ///
    /// - manufacturer
    /// - product
    /// - serial number
    /// - max_packet_size_0
    ///
    /// on the returned builder. If you change the manufacturer, product, or serial number fields,
    /// the test host may misbehave.
    pub fn make_device_builder<'a>(
        &self,
        usb_bus: &'a UsbBusAllocator<B>,
    ) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder::new(usb_bus, UsbVidPid(VID, PID), unsafe {
            CONTROL_BUFFER.get_mut()
        })
        .strings(&[StringDescriptors::default()
            .manufacturer(MANUFACTURER)
            .product(PRODUCT)
            .serial_number(SERIAL_NUMBER)])
        .unwrap()
        .max_packet_size_0(sizes::CONTROL_ENDPOINT)
        .unwrap()
    }

    /// Must be called after polling the UsbDevice.
    pub fn poll(&mut self) {
        if self.bench {
            match self.ep_bulk_out.read(&mut self.bulk_buf) {
                Ok(_) | Err(UsbError::WouldBlock) => {}
                Err(err) => panic!("bulk bench read {:?}", err),
            };

            match self
                .ep_bulk_in
                .write(&self.bulk_buf[0..self.ep_bulk_in.max_packet_size() as usize])
            {
                Ok(_) | Err(UsbError::WouldBlock) => {}
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
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("bulk read {:?}", err),
        };

        match self.ep_interrupt_out.read(&mut self.interrupt_buf) {
            Ok(count) => {
                if self.expect_interrupt_out {
                    self.expect_interrupt_out = false;
                } else {
                    panic!("unexpectedly read data from interrupt out endpoint");
                }

                self.ep_interrupt_in
                    .write(&self.interrupt_buf[0..count])
                    .expect("interrupt write");

                self.expect_interrupt_in_complete = true;
            }
            Err(UsbError::WouldBlock) => {}
            Err(err) => panic!("interrupt read {:?}", err),
        };
    }

    fn write_bulk_in(&mut self, write_empty: bool) {
        let to_write = cmp::min(
            self.len - self.i,
            self.ep_bulk_in.max_packet_size() as usize,
        );

        if to_write == 0 && !write_empty {
            self.len = 0;
            self.i = 0;

            return;
        }

        match self
            .ep_bulk_in
            .write(&self.bulk_buf[self.i..self.i + to_write])
        {
            Ok(count) => {
                assert_eq!(count, to_write);
                self.expect_bulk_in_complete = true;
                self.i += count;
            }
            Err(UsbError::WouldBlock) => {}
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
        writer.interface_alt(self.iface, 1, 0xff, 0x01, 0x00, Some(self.interface_string))?;
        writer.endpoint(&self.ep_iso_in)?;
        Ok(())
    }

    fn get_string(&self, index: StringIndex, lang_id: LangID) -> Option<&str> {
        if lang_id == LangID::EN_US {
            if index == self.custom_string {
                return Some(CUSTOM_STRING);
            } else if index == self.interface_string {
                return Some(INTERFACE_STRING);
            }
        }

        None
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
            REQ_READ_BUFFER if req.length as usize <= self.control_buf.len() => xfer
                .accept_with(&self.control_buf[0..req.length as usize])
                .expect("control_in REQ_READ_BUFFER failed"),
            REQ_READ_LONG_DATA => xfer
                .accept_with_static(LONG_DATA)
                .expect("control_in REQ_READ_LONG_DATA failed"),
            _ => xfer.reject().expect("control_in reject failed"),
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
                self.control_buf[0] =
                    (req.direction as u8) | (req.request_type as u8) << 5 | (req.recipient as u8);
                self.control_buf[1] = req.request;
                self.control_buf[2..4].copy_from_slice(&req.value.to_le_bytes());
                self.control_buf[4..6].copy_from_slice(&req.index.to_le_bytes());
                self.control_buf[6..8].copy_from_slice(&req.length.to_le_bytes());

                xfer.accept().expect("control_out REQ_STORE_REQUEST failed");
            }
            REQ_WRITE_BUFFER if xfer.data().len() <= self.control_buf.len() => {
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
