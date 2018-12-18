use core::cell::RefCell;
use crate::class_prelude::*;
use crate::device::{UsbDevice, UsbVidPid};
use crate::descriptor;

/// Test USB class for testing USB driver implementations. Driver implementations should include an
/// example called "test_class" that creates a device with this class.
pub struct TestClass<'a, B: UsbBus + 'a> {
    buffer: RefCell<[u8; 128]>,
    custom_string: StringIndex,
    ep_bulk_in: EndpointIn<'a, B>,
    ep_bulk_out: EndpointOut<'a, B>,
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
pub const REQ_UNKNOWN: u8 = 42;

impl<'a, B: UsbBus + 'a> TestClass<'a, B> {
    pub fn new(alloc: &UsbBusAllocator<B>) -> TestClass<'_, B> {
        TestClass {
            buffer: RefCell::new([0; 128]),
            custom_string: alloc.string(),
            ep_bulk_in: alloc.bulk(64),
            ep_bulk_out: alloc.bulk(64),
        }
    }

    pub fn make_device(&'a self, usb_bus: &'a UsbBusAllocator<B>)
        -> UsbDevice<'a, B>
    {
        UsbDevice::new(
                &usb_bus,
                UsbVidPid(VID, PID),
                &[self])
            .manufacturer(MANUFACTURER)
            .product(PRODUCT)
            .serial_number(SERIAL_NUMBER)
            .build()
    }

    pub fn poll(&self) {

    }
}

impl<'a, B: UsbBus + 'a> UsbClass<B> for TestClass<'a, B> {
    fn control_in(&self, xfer: ControlIn<B>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        let buf = self.buffer.borrow();

        match req.request {
            REQ_READ_BUFFER if req.length as usize <= buf.len()
                => xfer.accept_with(&buf[0..req.length as usize]).unwrap(),
            _ => xfer.reject().unwrap(),
        }
    }

    fn control_out(&self, xfer: ControlOut<B>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        let mut buf = self.buffer.borrow_mut();

        match req.request {
            REQ_STORE_REQUEST => {
                buf[0] = (req.direction as u8) | (req.request_type as u8) << 5 | (req.recipient as u8);
                buf[1] = req.request;
                buf[2..4].copy_from_slice(&req.value.to_le_bytes());
                buf[4..6].copy_from_slice(&req.index.to_le_bytes());
                buf[6..8].copy_from_slice(&req.length.to_le_bytes());

                xfer.accept().unwrap();
            },
            REQ_WRITE_BUFFER if xfer.data().len() as usize <= buf.len() => {
                assert_eq!(xfer.data().len(), req.length as usize, "xfer data len == req.length");

                buf[0..xfer.data().len()].copy_from_slice(xfer.data());

                xfer.accept().unwrap();
            }
            _ => xfer.reject().unwrap()
        }
    }

    fn get_string(&self, index: StringIndex, lang_id: u16) -> Option<&str> {
        if index == self.custom_string && lang_id == descriptor::lang_id::ENGLISH_US {
            Some(CUSTOM_STRING)
        } else {
            None
        }
    }
}

