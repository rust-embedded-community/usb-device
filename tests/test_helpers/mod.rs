use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use lazy_static::lazy_static;
use libusb::*;

pub use usb_device::test_class;

pub const TIMEOUT: Duration = Duration::from_secs(1);
pub const EN_US: u16 = 0x0409;

pub struct DeviceHandles<'a> {
    pub descriptor: DeviceDescriptor,
    pub handle: DeviceHandle<'a>,
    pub en_us: Language,
}

impl<'a> ::std::ops::Deref for DeviceHandles<'a> {
    type Target = DeviceHandle<'a>;

    fn deref(&self) -> &DeviceHandle<'a> {
        &self.handle
    }
}

impl<'a> ::std::ops::DerefMut for DeviceHandles<'a> {
    fn deref_mut(&mut self) -> &mut DeviceHandle<'a> {
        &mut self.handle
    }
}

lazy_static! {
    static ref CONTEXT: Context = Context::new().expect("create libusb context");
    static ref DEVICE: Mutex<DeviceHandles<'static>> = Mutex::new(open_device());
}

fn open_device() -> DeviceHandles<'static> {
    for device in CONTEXT.devices().expect("list devices").iter() {
        let descriptor = device.device_descriptor().expect("get device descriptor");

        if !(descriptor.vendor_id() == test_class::VID
            && descriptor.product_id() == test_class::PID) {
            continue;
        }

        let mut handle = device.open().expect("open device");

        let langs = handle.read_languages(TIMEOUT).expect("read languages");
        if langs.len() == 0 || langs[0].lang_id() != EN_US {
            continue;
        }

        let prod = handle.read_product_string(langs[0], &descriptor, TIMEOUT)
            .expect("read product string");

        if prod == test_class::PRODUCT {
            handle.reset().expect("reset device");

            return DeviceHandles {
                descriptor,
                handle,
                en_us: langs[0]
            };
        }
    }

    panic!("TestClass device not found");
}

pub fn get_device() -> MutexGuard<'static, DeviceHandles<'static>> {
    DEVICE.lock().unwrap()
}