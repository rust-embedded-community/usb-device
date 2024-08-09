use rusb::{ConfigDescriptor, Context, DeviceDescriptor, DeviceHandle, Language, UsbContext as _};
use std::thread;
use std::time::{Duration, Instant};
use usb_device::device::CONFIGURATION_VALUE;
use usb_device::test_class;

const TEST_INTERFACE: u8 = 0;

pub const TIMEOUT: Duration = Duration::from_secs(1);
pub const EN_US: u16 = 0x0409;

pub struct DeviceHandles {
    pub device_descriptor: DeviceDescriptor,
    pub config_descriptor: ConfigDescriptor,
    pub handle: DeviceHandle<Context>,
    pub en_us: Language,
}

impl DeviceHandles {
    /// Indicates if this device is (true) or isn't (false) a
    /// high-speed device.
    pub fn is_high_speed(&self) -> bool {
        self.handle.device().speed() == rusb::Speed::High
    }

    /// Returns the max packet size for the `TestClass` bulk endpoint(s).
    pub fn bulk_max_packet_size(&self) -> u16 {
        self.config_descriptor
            .interfaces()
            .flat_map(|intf| intf.descriptors())
            .flat_map(|desc| {
                desc.endpoint_descriptors()
                    .find(|ep| {
                        // Assumes that IN and OUT endpoint MPSes are the same.
                        ep.transfer_type() == rusb::TransferType::Bulk
                    })
                    .map(|ep| ep.max_packet_size())
            })
            .next()
            .expect("TestClass has at least one bulk endpoint")
    }

    /// Puts the device in a consistent state for running a test
    pub fn pre_test(&mut self) -> rusb::Result<()> {
        let res = self.reset();
        if let Err(err) = res {
            println!("Failed to reset the device: {}", err);
            return res;
        }

        let res = self.set_active_configuration(CONFIGURATION_VALUE);
        if let Err(err) = res {
            println!("Failed to set active configuration: {}", err);
            return res;
        }

        let res = self.claim_interface(TEST_INTERFACE);
        if let Err(err) = res {
            println!("Failed to claim interface: {}", err);
            return res;
        }

        Ok(())
    }

    /// Cleanup the device following a test
    pub fn post_test(&mut self) -> rusb::Result<()> {
        self.release_interface(TEST_INTERFACE)
    }
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

pub struct UsbContext {
    /// rusb Context handle
    inner: Context,
    device: Option<DeviceHandles>,
}

impl UsbContext {
    pub fn new() -> rusb::Result<Self> {
        let inner = rusb::Context::new()?;

        Ok(Self {
            inner,
            device: None,
        })
    }

    /// Attempt to open the test device once
    fn open_device_immediate(&self) -> rusb::Result<DeviceHandles> {
        for device in self.inner.devices()?.iter() {
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

    /// Look for the device, retry until timeout expires
    pub fn open_device(&mut self, timeout: Option<Duration>) -> rusb::Result<&mut DeviceHandles> {
        if self.device.is_none() {
            match timeout {
                Some(timeout) => {
                    let deadline = Instant::now() + timeout;
                    loop {
                        if let Ok(dev) = self.open_device_immediate() {
                            self.device = Some(dev);
                            break;
                        }
                        let now = Instant::now();
                        if now >= deadline {
                            break;
                        } else {
                            let dur = Duration::from_millis(100).min(deadline - now);
                            thread::sleep(dur);
                        }
                    }
                }
                None => {
                    if let Ok(dev) = self.open_device_immediate() {
                        self.device = Some(dev);
                    }
                }
            }
        }

        match self.device.as_mut() {
            Some(device) => Ok(device),
            None => Err(rusb::Error::NoDevice),
        }
    }

    /// Closes device if it was open (handling errors), attempts to reopen
    pub fn reopen_device(&mut self, timeout: Option<Duration>) -> rusb::Result<&mut DeviceHandles> {
        // This is expected to fail in tests where device was asked to reset
        let _ = self.cleanup_after_test();

        self.device = None;

        self.open_device(timeout)
    }

    /// Attempts to open (if necessary) and (re-)initialize a device for a test
    pub fn device_for_test(&mut self) -> rusb::Result<&mut DeviceHandles> {
        let dev = match self.open_device(Some(Duration::from_secs(5))) {
            Ok(dev) => dev,
            Err(err) => {
                println!("Did not find a TestClass device. Make sure the device is correctly programmed and plugged in. Last error: {}", err);
                return Err(err);
            }
        };

        match dev.pre_test() {
            Ok(()) => Ok(dev),
            Err(err) => {
                println!("Failed to prepare for test: {}", err);
                Err(err)
            }
        }
    }

    /// Releases resources that might have been used in a test
    pub fn cleanup_after_test(&mut self) -> rusb::Result<()> {
        if let Some(dev) = &mut self.device {
            dev.post_test()
        } else {
            Ok(())
        }
    }
}
