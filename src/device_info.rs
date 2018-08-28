// Turn this into a builder for validation

pub struct UsbDeviceInfo<'a> {
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_release: u16,
    pub manufacturer: &'a str,
    pub product: &'a str,
    pub serial_number: &'a str,
    pub self_powered: bool,
    pub remote_wakeup: bool,
    pub max_power: u8,
}

impl<'a> UsbDeviceInfo<'a> {
    pub fn new(vendor_id: u16, product_id: u16) -> UsbDeviceInfo<'a> {
        UsbDeviceInfo {
            device_class: 0x00,
            device_sub_class: 0x00,
            device_protocol: 0x00,
            max_packet_size_0: 8,
            vendor_id,
            product_id,
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

