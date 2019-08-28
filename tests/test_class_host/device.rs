use std::time::Duration;
use libusb::*;
use usb_device::test_class;

pub const TIMEOUT: Duration = Duration::from_secs(1);
pub const EN_US: u16 = 0x0409;

pub struct DeviceHandles<'a> {
    pub device_descriptor: DeviceDescriptor,
    pub config_descriptor: ConfigDescriptor,
    pub handle: DeviceHandle<'a>,
    pub en_us: Language,
    pub ep_bulk_in: u8,
    pub ep_bulk_out: u8,
    pub ep_interrupt_in: u8,
    pub ep_interrupt_out: u8,
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

pub fn open_device(ctx: &Context) -> libusb::Result<DeviceHandles<'_>> {
    for device in ctx.devices()?.iter() {
        let device_descriptor = device.device_descriptor()?;

        if !(device_descriptor.vendor_id() == test_class::VID
            && device_descriptor.product_id() == test_class::PID) {
            continue;
        }

        let mut handle = device.open()?;

        let langs = handle.read_languages(TIMEOUT)?;
        if langs.len() == 0 || langs[0].lang_id() != EN_US {
            continue;
        }

        let prod = handle.read_product_string(langs[0], &device_descriptor, TIMEOUT)?;

        if prod == test_class::PRODUCT {
            handle.reset()?;

            let config_descriptor = device.config_descriptor(0)?;

            let mut endpoints: Vec<(Direction, TransferType, u8)> = Vec::new();
            for interface in config_descriptor.interfaces() {
                for interface_descriptor in interface.descriptors() {
                    for ep in interface_descriptor.endpoint_descriptors() {
                        endpoints.push((ep.direction(), ep.transfer_type(), ep.address()));
                    }
                }
            }

            /*let endpoints = config_descriptor.interfaces()
                .flat_map(|i| i.descriptors())
                .flat_map(|d| d.endpoint_descriptors())
                .collect::<Vec<_>>();*/

            let get_ep = |dir: Direction, ep_type: TransferType| {
                endpoints.iter()
                    .find(|ep| ep.0 == dir && ep.1 == ep_type)
                    .unwrap()
                    .2
            };

            return Ok(DeviceHandles {
                ep_bulk_in: get_ep(Direction::In, TransferType::Bulk),
                ep_bulk_out: get_ep(Direction::Out, TransferType::Bulk),
                ep_interrupt_in: get_ep(Direction::In, TransferType::Interrupt),
                ep_interrupt_out: get_ep(Direction::Out, TransferType::Interrupt),
                device_descriptor,
                config_descriptor,
                handle,
                en_us: langs[0],
            });
        }
    }

    Err(libusb::Error::NoDevice)
}
