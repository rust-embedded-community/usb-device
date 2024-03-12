use crate::bus::{UsbBus, UsbBusAllocator};
use crate::descriptor::lang_id::LangID;
use crate::device::{Config, UsbDevice, UsbRev};

/// A USB vendor ID and product ID pair.
pub struct UsbVidPid(pub u16, pub u16);

/// Used to build new [`UsbDevice`]s.
pub struct UsbDeviceBuilder<'a, B: UsbBus> {
    alloc: &'a UsbBusAllocator<B>,
    control_buffer: &'a mut [u8],
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

#[derive(Copy, Clone, Debug, PartialEq)]
/// Error type for the USB device builder
pub enum BuilderError {
    /// String descriptors were provided in more languages than are supported
    TooManyLanguages,
    /// Control endpoint can only be 8, 16, 32, or 64 byte max packet size
    InvalidPacketSize,
    /// Configuration specifies higher USB power draw than allowed
    PowerTooHigh,
    /// The provided control buffer is too small for the provided maximum packet size.
    ControlBufferTooSmall,
}

/// Provides basic string descriptors about the device, including the manufacturer, product name,
/// and serial number of the device in a specified language.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StringDescriptors<'a> {
    pub(crate) id: LangID,
    pub(crate) serial: Option<&'a str>,
    pub(crate) product: Option<&'a str>,
    pub(crate) manufacturer: Option<&'a str>,
}

impl<'a> Default for StringDescriptors<'a> {
    fn default() -> Self {
        Self::new(LangID::EN_US)
    }
}

impl<'a> StringDescriptors<'a> {
    /// Create a new descriptor list with the provided language.
    pub fn new(lang_id: LangID) -> Self {
        Self {
            id: lang_id,
            serial: None,
            product: None,
            manufacturer: None,
        }
    }

    /// Specify the serial number for this language.
    pub fn serial_number(mut self, serial: &'a str) -> Self {
        self.serial.replace(serial);
        self
    }

    /// Specify the manufacturer name for this language.
    pub fn manufacturer(mut self, manufacturer: &'a str) -> Self {
        self.manufacturer.replace(manufacturer);
        self
    }

    /// Specify the product name for this language.
    pub fn product(mut self, product: &'a str) -> Self {
        self.product.replace(product);
        self
    }
}

impl<'a, B: UsbBus> UsbDeviceBuilder<'a, B> {
    /// Creates a builder for constructing a new [`UsbDevice`].
    pub fn new(
        alloc: &'a UsbBusAllocator<B>,
        vid_pid: UsbVidPid,
        control_buffer: &'a mut [u8],
    ) -> UsbDeviceBuilder<'a, B> {
        UsbDeviceBuilder {
            alloc,
            control_buffer,
            config: Config {
                device_class: 0x00,
                device_sub_class: 0x00,
                device_protocol: 0x00,
                max_packet_size_0: 8,
                vendor_id: vid_pid.0,
                product_id: vid_pid.1,
                usb_rev: UsbRev::Usb210,
                device_release: 0x0010,
                string_descriptors: heapless::Vec::new(),
                self_powered: false,
                supports_remote_wakeup: false,
                composite_with_iads: false,
                max_power: 50,
            },
        }
    }

    /// Creates the [`UsbDevice`] instance with the configuration in this builder.
    pub fn build(self) -> Result<UsbDevice<'a, B>, BuilderError> {
        if self.control_buffer.len() < self.config.max_packet_size_0 as usize {
            return Err(BuilderError::ControlBufferTooSmall);
        }

        Ok(UsbDevice::build(
            self.alloc,
            self.config,
            self.control_buffer,
        ))
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

    /// Specify the strings for the device.
    ///
    /// # Note
    /// Up to 16 languages may be provided.
    pub fn strings(mut self, descriptors: &[StringDescriptors<'a>]) -> Result<Self, BuilderError> {
        // The 16 language limit comes from the size of the buffer used to provide the list of
        // language descriptors to the host.
        self.config.string_descriptors =
            heapless::Vec::from_slice(descriptors).map_err(|_| BuilderError::TooManyLanguages)?;

        Ok(self)
    }

    /// Sets the maximum packet size in bytes for the control endpoint 0.
    ///
    /// Valid values are 8, 16, 32 and 64. There's generally no need to change this from the default
    /// value of 8 bytes unless a class uses control transfers for sending large amounts of data, in
    /// which case using a larger packet size may be more efficient.
    ///
    /// Default: 8 bytes
    pub fn max_packet_size_0(mut self, max_packet_size_0: u8) -> Result<Self, BuilderError> {
        match max_packet_size_0 {
            8 | 16 | 32 | 64 => {}
            _ => return Err(BuilderError::InvalidPacketSize),
        }

        if self.control_buffer.len() < max_packet_size_0 as usize {
            return Err(BuilderError::ControlBufferTooSmall);
        }

        self.config.max_packet_size_0 = max_packet_size_0;
        Ok(self)
    }

    /// Sets the maximum current drawn from the USB bus by the device in milliamps.
    ///
    /// The default is 100 mA. If your device always uses an external power source and never draws
    /// power from the USB bus, this can be set to 0.
    ///
    /// See also: `self_powered`
    ///
    /// Default: 100mA
    pub fn max_power(mut self, max_power_ma: usize) -> Result<Self, BuilderError> {
        if max_power_ma > 500 {
            return Err(BuilderError::PowerTooHigh);
        }

        self.config.max_power = (max_power_ma / 2) as u8;
        Ok(self)
    }
}
