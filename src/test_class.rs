use core::cell::RefCell;
use core::cmp;
use crate::Result;
use crate::class_prelude::*;
use crate::device::{UsbDevice, UsbVidPid};
use crate::descriptor;

/// Test USB class for testing USB driver implementations. Driver implementations should include an
/// example called "test_class" that creates a device with this class.
pub struct TestClass<'a, B: UsbBus> {
    state: RefCell<State>,
    custom_string: StringIndex,
    iface: InterfaceNumber,
    ep_bulk_in: EndpointIn<'a, B>,
    ep_bulk_out: EndpointOut<'a, B>,
    ep_interrupt_in: EndpointIn<'a, B>,
    ep_interrupt_out: EndpointOut<'a, B>,
}

struct State {
    control_buf: [u8; 256],
    bulk_buf: [u8; 256],
    interrupt_buf: [u8; 256],
    len: usize,
    i: usize,
    expect_bulk_in_complete: bool,
    expect_bulk_out: bool,
    expect_interrupt_in_complete: bool,
    expect_interrupt_out: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            control_buf: [0; 256],
            bulk_buf: [0; 256],
            interrupt_buf: [0; 256],
            len: 0,
            i: 0,
            expect_bulk_in_complete: false,
            expect_bulk_out: false,
            expect_interrupt_in_complete: false,
            expect_interrupt_out: false,
        }
    }
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


impl<B: UsbBus> TestClass<'_, B> {
    pub fn new(alloc: &UsbBusAllocator<B>) -> TestClass<'_, B> {
        TestClass {
            state: RefCell::default(),
            custom_string: alloc.string(),
            iface: alloc.interface(),
            ep_bulk_in: alloc.bulk(64),
            ep_bulk_out: alloc.bulk(64),
            ep_interrupt_in: alloc.interrupt(31, 1),
            ep_interrupt_out: alloc.interrupt(31, 1),
        }
    }

    pub fn make_device<'a>(&'a self, usb_bus: &'a UsbBusAllocator<B>) -> UsbDevice<'a, B>
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
        let mut s = self.state.borrow_mut();

        let i = s.i;
        match self.ep_bulk_out.read(&mut s.bulk_buf[i..]) {
            Ok(count) => {
                if s.expect_bulk_out {
                    s.expect_bulk_out = false;
                } else {
                    panic!("unexpectedly read data from bulk out endpoint");
                }

                s.i += count;

                if count < self.ep_bulk_out.max_packet_size() as usize {
                    s.len = s.i;
                    s.i = 0;//

                    self.write_bulk_in(&mut s, count == 0);
                }
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("bulk read {:?}", err),
        };

        match self.ep_interrupt_out.read(&mut s.interrupt_buf) {
            Ok(count) => {
                if s.expect_interrupt_out {
                    s.expect_interrupt_out = false;
                } else {
                    panic!("unexpectedly read data from interrupt out endpoint");
                }

                self.ep_interrupt_in.write(&s.interrupt_buf[0..count])
                    .expect("interrupt write");

                s.expect_interrupt_in_complete = true;
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("bulk read {:?}", err),
        };
    }

    fn write_bulk_in(&self, s: &mut State, write_empty: bool) {
        let to_write = cmp::min(s.len - s.i, self.ep_bulk_in.max_packet_size() as usize);

        if to_write == 0 && !write_empty {
            s.len = 0;
            s.i = 0;

            return;
        }

        match self.ep_bulk_in.write(&s.bulk_buf[s.i..s.i+to_write]) {
            Ok(count) => {
                assert_eq!(count, to_write);
                s.expect_bulk_in_complete = true;
                s.i += count;
            },
            Err(UsbError::WouldBlock) => { },
            Err(err) => panic!("bulk write {:?}", err),
        };
    }
}

impl<B: UsbBus> UsbClass<B> for TestClass<'_, B> {
    fn reset(&self) -> Result<()> {
        *self.state.borrow_mut() = Default::default();

        Ok(())
    }

    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface(self.iface, 0xff, 0x00, 0x00)?;
        writer.endpoint(&self.ep_bulk_in)?;
        writer.endpoint(&self.ep_bulk_out)?;
        writer.endpoint(&self.ep_interrupt_in)?;
        writer.endpoint(&self.ep_interrupt_out)?;

        Ok(())
    }

    fn endpoint_in_complete(&self, addr: EndpointAddress) {
        let mut s = self.state.borrow_mut();

        if addr == self.ep_bulk_in.address() {
            if s.expect_bulk_in_complete {
                s.expect_bulk_in_complete = false;

                self.write_bulk_in(&mut s, false);
            } else {
                panic!("unexpected endpoint_in_complete");
            }
        } else if addr == self.ep_interrupt_in.address() {
            if s.expect_interrupt_in_complete {
                s.expect_interrupt_in_complete = false;
            } else {
                panic!("unexpected endpoint_in_complete");
            }
        }
    }

    fn endpoint_out(&self, addr: EndpointAddress) {
        let mut s = self.state.borrow_mut();

        if addr == self.ep_bulk_out.address() {
            s.expect_bulk_out = true;
        } else if addr == self.ep_interrupt_out.address() {
            s.expect_interrupt_out = true;
        }
    }

    fn control_in(&self, xfer: ControlIn<B>) {
        let req = *xfer.request();

        if !(req.request_type == control::RequestType::Vendor
            && req.recipient == control::Recipient::Device)
        {
            return;
        }

        let s = self.state.borrow_mut();

        match req.request {
            REQ_READ_BUFFER if req.length as usize <= s.control_buf.len()
                => xfer.accept_with(&s.control_buf[0..req.length as usize]).unwrap(),
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

        let mut s = self.state.borrow_mut();

        match req.request {
            REQ_STORE_REQUEST => {
                s.control_buf[0] = (req.direction as u8) | (req.request_type as u8) << 5 | (req.recipient as u8);
                s.control_buf[1] = req.request;
                s.control_buf[2..4].copy_from_slice(&req.value.to_le_bytes());
                s.control_buf[4..6].copy_from_slice(&req.index.to_le_bytes());
                s.control_buf[6..8].copy_from_slice(&req.length.to_le_bytes());

                xfer.accept().unwrap();
            },
            REQ_WRITE_BUFFER if xfer.data().len() as usize <= s.control_buf.len() => {
                assert_eq!(xfer.data().len(), req.length as usize, "xfer data len == req.length");

                s.control_buf[0..xfer.data().len()].copy_from_slice(xfer.data());

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

