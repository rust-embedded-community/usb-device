use crate::bus::{UsbBus, UsbBusAllocator};
use crate::descriptor::lang_id::LangID;
use crate::device::{Config, UsbDevice, UsbRev};

/// A USB vendor ID and product ID pair.
pub struct UsbVidPid(pub u16, pub u16);

/// Used to build new [`UsbDevice`]s.
pub struct UsbDeviceBuilder<'a, B: UsbBus> {
    alloc: &'a UsbBusAllocator<B>,
    config: Config<'a>,
}

macro_rules! builder_fields {
    ( $( $(#[$meta:meta])* $name:ident: $type:ty, )* ) => {
        $(
            $(#[$meta])*
            pub fn $name(mut self, $name: $type) -> Self {
                self.config.$name = $name;
                self
            }
        )*
    }
}

impl<'a, B: UsbBus> UsbDeviceBuilder<'a, B> {
    /// Creates a builder for constructing a new [`UsbDevice`].
    pub fn new(alloc: &'a UsbBusAllocator<B>, vid_pid: UsbVidPid) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder {
            alloc,
            config: Config {
                device_class: 0x00,
                device_sub_class: 0x00,
                device_protocol: 0x00,
                max_packet_size_0: 8,
                vendor_id: vid_pid.0,
                product_id: vid_pid.1,
                usb_rev: UsbRev::Usb210,
                device_release: 0x0010,
                extra_lang_ids: None,
                manufacturer: None,
                product: None,
                serial_number: None,
                self_powered: false,
                supports_remote_wakeup: false,
                composite_with_iads: false,
                max_power: 50,
            },
        }
    }

    /// Creates the [`UsbDevice`] instance with the configuration in this builder.
    pub fn build(self) -> UsbDevice<'a, B> {
        UsbDevice::build(self.alloc, self.config)
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

        /// Sets which Usb 2 revision to comply to.
        ///
        /// Default: `UsbRev::Usb210`
        usb_rev: UsbRev,
    }

    /// Configures the device as a composite device with interface association descriptors.
    pub fn composite_with_iads(mut self) -> Self {
        // Magic values specified in USB-IF ECN on IADs.
        self.config.device_class = 0xEF;
        self.config.device_sub_class = 0x02;
        self.config.device_protocol = 0x01;

        self.config.composite_with_iads = true;
        self
    }

    /// Sets **extra** Language ID for device.
    ///
    /// Since "en_US"(0x0409) is implicitly embedded, you just need to fill other LangIDs
    ///
    /// Default: (none)
    pub fn set_extra_lang_ids(mut self, extra_lang_ids: &'a [LangID]) -> Self {
        if extra_lang_ids.len() == 0 {
            self.config.extra_lang_ids = None;
            return self;
        }

        assert!(
            extra_lang_ids.len() < 16,
            "Not support more than 15 extra LangIDs"
        );

        [
            self.config.manufacturer,
            self.config.product,
            self.config.serial_number,
        ]
        .iter()
        .zip(["manufacturer", "product", "serial_number"].iter())
        .for_each(|(list, field_name)| {
            // do list length check only if user already specify "manufacturer", "product" or "serial_number"
            if let Some(list) = list {
                assert!(
                    extra_lang_ids.len() == list.len() - 1,
                    "The length of \"extra_lang_id\" list should be one less than \"{}\" list",
                    field_name
                )
            }
        });

        self.config.extra_lang_ids = Some(extra_lang_ids);

        self
    }

    /// Sets the manufacturer name string descriptor.
    ///
    /// the first string should always be in English, the language of rest strings
    /// should be pair with what inside [.extra_lang_ids()](Self::extra_lang_ids)
    ///
    /// Default: (none)
    pub fn manufacturer(mut self, manufacturer_ls: &'a [&'a str]) -> Self {
        if manufacturer_ls.len() == 0 {
            self.config.manufacturer = None;
            return self;
        }

        assert!(
            manufacturer_ls.len() <= 16,
            "Not support more than 16 \"manufacturer\"s"
        );

        let num_extra_langs = self
            .config
            .extra_lang_ids
            .as_ref()
            .map(|langs| langs.len())
            .unwrap_or(0);

        assert!(
            manufacturer_ls.len() == num_extra_langs + 1,
            "The length of \"manufacturer\" list should be one more than \"extra_lang_ids\" list",
        );

        self.config.manufacturer = Some(manufacturer_ls);

        self
    }

    /// Sets the product name string descriptor.
    ///
    /// the first string should always be in English, the language of rest strings
    /// should be pair with what inside [.extra_lang_ids()](Self::extra_lang_ids)
    ///
    /// Default: (none)
    pub fn product(mut self, product_ls: &'a [&'a str]) -> Self {
        if product_ls.len() == 0 {
            self.config.product = None;
            return self;
        }

        assert!(
            product_ls.len() <= 16,
            "Not support more than 16 \"product\"s"
        );

        let num_extra_langs = self
            .config
            .extra_lang_ids
            .as_ref()
            .map(|langs| langs.len())
            .unwrap_or(0);

        assert!(
            product_ls.len() == num_extra_langs + 1,
            "The length of \"product\" list should be one more than \"extra_lang_ids\" list",
        );

        self.config.product = Some(product_ls);

        self
    }

    /// Sets the serial number string descriptor.
    ///
    /// the first string should always be in English, the language of rest strings
    /// should be pair with what inside [.extra_lang_ids()](Self::extra_lang_ids)
    ///
    /// Default: (none)
    pub fn serial_number(mut self, serial_number_ls: &'a [&'a str]) -> Self {
        if serial_number_ls.len() == 0 {
            self.config.serial_number = None;
            return self;
        }

        assert!(
            serial_number_ls.len() <= 16,
            "Not support more than 16 \"serial_number\"s"
        );

        let num_extra_langs = self
            .config
            .extra_lang_ids
            .as_ref()
            .map(|langs| langs.len())
            .unwrap_or(0);

        assert!(
            serial_number_ls.len() == num_extra_langs + 1,
            "The length of \"serial_number\" list should be one more than \"extra_lang_ids\" list",
        );

        self.config.serial_number = Some(serial_number_ls);

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

        self.config.max_packet_size_0 = max_packet_size_0;
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

        self.config.max_power = (max_power_ma / 2) as u8;
        self
    }
}
