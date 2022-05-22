use rusb::{ConfigDescriptor, Context, DeviceDescriptor, DeviceHandle, Language, UsbContext as _};
use std::time::Duration;
use usb_device::test_class;

pub const TIMEOUT: Duration = Duration::from_secs(1);
pub const EN_US: u16 = 0x0409;

pub struct DeviceHandles {
    pub device_descriptor: DeviceDescriptor,
    pub config_descriptor: ConfigDescriptor,
    pub handle: DeviceHandle<Context>,
    pub en_us: Language,
}

impl ::std::ops::Deref for DeviceHandles {
    type Target = DeviceHandle<Context>;

    fn deref(&self) -> &DeviceHandle<Context> {
        &self.handle
    }
}

impl ::std::ops::DerefMut for DeviceHandles {
    fn deref_mut(&mut self) -> &mut DeviceHandle<Context> {
        &mut self.handle
    }
}

pub fn open_device(ctx: &Context) -> rusb::Result<DeviceHandles> {
    for device in ctx.devices()?.iter() {
        let device_descriptor = device.device_descriptor()?;

        if !(device_descriptor.vendor_id() == test_class::VID
            && device_descriptor.product_id() == test_class::PID)
        {
            continue;
        }

        let mut handle = device.open()?;

        let langs = handle.read_languages(TIMEOUT)?;
        if langs.is_empty() || langs[0].lang_id() != EN_US {
            continue;
        }

        let prod = handle.read_product_string(langs[0], &device_descriptor, TIMEOUT)?;

        if prod == test_class::PRODUCT {
            handle.reset()?;

            let config_descriptor = device.config_descriptor(0)?;

            return Ok(DeviceHandles {
                device_descriptor,
                config_descriptor,
                handle,
                en_us: langs[0],
            });
        }
    }

    Err(rusb::Error::NoDevice)
}
