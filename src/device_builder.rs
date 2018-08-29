use bus::UsbBus;
use device::{UsbDevice, UsbDeviceInfo};
use class::UsbClass;

pub struct UsbVidPid(pub u16, pub u16);

pub struct UsbDeviceBuilder<'a, B: 'a> {
    bus: &'a B,
    info: UsbDeviceInfo<'a>,
}

macro_rules! builder_fields {
    ($($name:ident: $type:ty,)*) => {
        $(
            pub fn $name(&mut self, $name: $type) -> &mut Self {
                self.info.$name = $name;
                self
            }
        )*
    }
}

impl<'a, B: 'a + UsbBus> UsbDeviceBuilder<'a, B> {
    pub(crate) fn new(bus: &'a B, vid_pid: UsbVidPid) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder {
            bus,
            info: UsbDeviceInfo {
                device_class: 0x00,
                device_sub_class: 0x00,
                device_protocol: 0x00,
                max_packet_size_0: 8,
                vendor_id: vid_pid.0,
                product_id: vid_pid.1,
                device_release: 0x0010,
                manufacturer: "",
                product: "",
                serial_number: "",
                self_powered: false,
                remote_wakeup: false,
                max_power: 50,
            }
        }
    }

    builder_fields! {
        device_class: u8,
        device_sub_class: u8,
        device_protocol: u8,
        device_release: u16,
        manufacturer: &'a str,
        product: &'a str,
        serial_number: &'a str,
        self_powered: bool,
        remote_wakeup: bool,
    }

    pub fn max_packet_size_0(&mut self, max_packet_size_0: u8) -> &mut Self {
        match max_packet_size_0 {
            8 | 16 | 32 | 64 => { }
            _ => panic!("invalid max_packet_size_0")
        }

        self.info.max_packet_size_0 = max_packet_size_0;
        self
    }

    pub fn max_power(&mut self, max_power_ma: usize) -> &mut Self {
        if max_power_ma > 500 {
            panic!("max_power is too much")
        }

        self.info.max_power = (max_power_ma / 2) as u8;
        self
    }

    pub fn build(&self, classes: &[&'a dyn UsbClass]) -> UsbDevice<'a, B> {
        UsbDevice::build(self.bus, classes, self.info)
    }
}