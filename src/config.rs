/// A USB vendor ID and product ID pair.
pub struct UsbVidPid(pub u16, pub u16);

macro_rules! builder_fields {
    ( $( $(#[$meta:meta])* $name:ident: $type:ty, )* ) => {
        $(
            $(#[$meta])*
            pub fn $name(mut self, $name: $type) -> Self {
                self.$name = $name;
                self
            }
        )*
    }
}

/// A USB device configuration.
#[derive(Clone, Copy, Debug)]
pub struct Config<'a> {
    /// USB device class
    pub device_class: u8,
    /// USB device subclass
    pub device_sub_class: u8,
    /// USB device protocol
    pub device_protocol: u8,
    /// USB control endpoint maximum packet size
    pub max_packet_size_0: u8,
    /// USB vendor ID
    pub vendor_id: u16,
    /// USB product ID
    pub product_id: u16,
    /// BCD encoded device release
    pub device_release: u16,
    /// Manufacturer string
    pub manufacturer: Option<&'a str>,
    /// Product string
    pub product: Option<&'a str>,
    /// Serial number
    pub serial_number: Option<&'a str>,
    /// Is device self-powered?
    pub self_powered: bool,
    /// Does device support remote wakeup?
    pub supports_remote_wakeup: bool,
    /// Is device composite with IADs?
    pub composite_with_iads: bool,
    /// Maximum power consumption of device
    pub max_power: u8,
}

impl<'a> Config<'a> {

    /// Create a USB device configuration with given vendor and product ID,
    /// using defaults.
    pub fn new(vid_pid: UsbVidPid) -> Self {
        Self {
            device_class: 0x00,
            device_sub_class: 0x00,
            device_protocol: 0x00,
            max_packet_size_0: 8,
            vendor_id: vid_pid.0,
            product_id: vid_pid.1,
            device_release: 0x0010,
            manufacturer: None,
            product: None,
            serial_number: None,
            self_powered: false,
            supports_remote_wakeup: false,
            composite_with_iads: false,
            max_power: 50,
        }
    }

    builder_fields! {
        /// Sets the device class code assigned by USB.org. Set to `0xff` for vendor-specific
        /// devices that do not conform to any class.
        ///
        /// Default: `0x00` (class code specified by interfaces)
        device_class: u8,

        /// Sets the device sub-class code. Depends on class.
        ///
        /// Default: `0x00`
        device_sub_class: u8,

        /// Sets the device protocol code. Depends on class and sub-class.
        ///
        /// Default: `0x00`
        device_protocol: u8,

        /// Sets the device release version in BCD.
        ///
        /// Default: `0x0010` ("0.1")
        device_release: u16,

        /// Sets whether the device may have an external power source.
        ///
        /// This should be set to `true` even if the device is sometimes self-powered and may not
        /// always draw power from the USB bus.
        ///
        /// Default: `false`
        ///
        /// See also: `max_power`
        self_powered: bool,

        /// Sets whether the device supports remotely waking up the host is requested.
        ///
        /// Default: `false`
        supports_remote_wakeup: bool,
    }

    /// Configures the device as a composite device with interface association descriptors.
    pub fn composite_with_iads(mut self) -> Self {
        // Magic values specified in USB-IF ECN on IADs.
        self.device_class = 0xEF;
        self.device_sub_class = 0x02;
        self.device_protocol = 0x01;

        self.composite_with_iads = true;
        self
    }

    /// Sets the manufacturer name string descriptor.
    ///
    /// Default: (none)
    pub fn manufacturer(mut self, manufacturer: &'a str) -> Self {
        self.manufacturer = Some(manufacturer);
        self
    }

    /// Sets the product name string descriptor.
    ///
    /// Default: (none)
    pub fn product(mut self, product: &'a str) -> Self {
        self.product = Some(product);
        self
    }

    /// Sets the serial number string descriptor.
    ///
    /// Default: (none)
    pub fn serial_number(mut self, serial_number: &'a str) -> Self {
        self.serial_number = Some(serial_number);
        self
    }

    /// Sets the maximum packet size in bytes for the control endpoint 0.
    ///
    /// Valid values are 8, 16, 32 and 64. There's generally no need to change this from the default
    /// value of 8 bytes unless a class uses control transfers for sending large amounts of data, in
    /// which case using a larger packet size may be more efficient.
    ///
    /// Default: 8 bytes
    pub fn max_packet_size_0(mut self, max_packet_size_0: u8) -> Self {
        match max_packet_size_0 {
            8 | 16 | 32 | 64 => {}
            _ => panic!("invalid max_packet_size_0"),
        }

        self.max_packet_size_0 = max_packet_size_0;
        self
    }

    /// Sets the maximum current drawn from the USB bus by the device in milliamps.
    ///
    /// The default is 100 mA. If your device always uses an external power source and never draws
    /// power from the USB bus, this can be set to 0.
    ///
    /// See also: `self_powered`
    ///
    /// Default: 100mA
    pub fn max_power(mut self, max_power_ma: usize) -> Self {
        if max_power_ma > 500 {
            panic!("max_power is too much")
        }

        self.max_power = (max_power_ma / 2) as u8;
        self
    }
}
