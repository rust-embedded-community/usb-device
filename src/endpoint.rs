use crate::usbcore::{UsbCore, UsbEndpoint, UsbEndpointIn, UsbEndpointOut};
use crate::{Result, UsbDirection, UsbError};

/// USB endpoint transfer type. The values of this enum can be directly cast into `u8` to get the
/// transfer bmAttributes transfer type bits.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EndpointType {
    /// Control endpoint. Used for device management. Only the host can initiate requests. Usually
    /// used only endpoint 0.
    Control = 0b00,

    /// Isochronous endpoint. Used for time-critical unreliable data. Not implemented yet.
    Isochronous = 0b01,

    /// Bulk endpoint. Used for large amounts of best-effort reliable data.
    Bulk = 0b10,

    /// Interrupt endpoint. Used for small amounts of time-critical reliable data.
    Interrupt = 0b11,
}

/// USB endpoint configuration
pub struct EndpointConfig {
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
    fixed_address: Option<EndpointAddress>,
}

impl EndpointConfig {
    pub const fn control(max_packet_size: u16) -> Self {
        Self {
            ep_type: EndpointType::Control,
            max_packet_size,
            interval: 0,
            fixed_address: None,
        }
    }

    pub const fn bulk(max_packet_size: u16) -> Self {
        Self {
            ep_type: EndpointType::Bulk,
            max_packet_size,
            interval: 0,
            fixed_address: None,
        }
    }

    pub const fn interrupt(max_packet_size: u16, interval: u8) -> Self {
        Self {
            ep_type: EndpointType::Interrupt,
            max_packet_size,
            interval,
            fixed_address: None,
        }
    }

    pub const fn with_fixed_address(self, address: EndpointAddress) -> Self {
        Self {
            fixed_address: Some(address),
            ..self
        }
    }

    pub const fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    pub const fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    pub const fn interval(&self) -> u8 {
        self.interval
    }

    pub const fn fixed_address(&self) -> Option<EndpointAddress> {
        self.fixed_address
    }
}

// TODO: maybe make a const "into()"

impl<U: UsbCore> From<EndpointConfig> for EndpointOut<U> {
    fn from(config: EndpointConfig) -> Self {
        EndpointOut { config, core: None }
    }
}

impl<U: UsbCore> From<EndpointConfig> for EndpointIn<U> {
    fn from(config: EndpointConfig) -> Self {
        EndpointIn { config, core: None }
    }
}

pub(crate) struct EndpointCore<EP> {
    pub(crate) enabled: bool,
    pub(crate) ep: EP,
}

pub struct EndpointOut<U: UsbCore> {
    pub(crate) config: EndpointConfig,
    pub(crate) core: Option<EndpointCore<U::EndpointOut>>,
}

impl<U: UsbCore> EndpointOut<U> {
    pub fn ep_type(&self) -> EndpointType {
        self.config.ep_type
    }

    pub fn max_packet_size(&self) -> u16 {
        self.config.max_packet_size
    }

    pub fn interval(&self) -> u8 {
        self.config.interval
    }

    pub fn address(&self) -> EndpointAddress {
        self.core
            .as_ref()
            .map(|c| c.ep.address())
            .unwrap_or(EndpointAddress(0))
    }

    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet. The buffer should be large enough to fit at least as many bytes as the
    /// `max_packet_size` specified when allocating the endpoint.
    ///
    /// Packets are read in the order they arrive. Peripheral implementations may have a receive
    /// buffer that fits multiple packets per endpoint. This method is only valid for non-control
    /// endpoints, but normal class implementations do not have control endpoints.
    ///
    /// # Errors
    ///
    /// Note: USB bus implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read. Note that
    ///   this is different from a received zero-length packet, which is valid and significant in
    ///   USB. A zero-length packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `data`. This is generally an error in the class implementation.
    /// * [`EndpointDisabled`](crate::UsbError::EndpointDisabled) - The endpoint is not currently
    ///   enabled, due to the device not being configured, or the endpoint belonging to an inactive
    ///   interface alternate setting.
    /// * [`Unsupported`](crate::UsbError::Unsupported) - The endpoint is a control endpoint.
    ///   Control endpoints must use [`control_read_packet`].
    pub fn read_packet(&mut self, data: &mut [u8]) -> Result<usize> {
        self.control_read_packet(data).map(|(count, _)| count)
    }

    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet and whether it was a DATA or SETUP packet. The buffer should be large enough to
    /// fit at least as many bytes as the `max_packet_size` specified when allocating the endpoint.
    /// In practice, this method should never be needed by classes, unless trying to be compatible
    /// with a weird existing class.
    ///
    /// # Errors
    ///
    /// Note: USB bus implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read. Note that
    ///   this is different from a received zero-length packet, which is valid and significant in
    ///   USB. A zero-length packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `data`. This is generally an error in the class implementation.
    /// * [`EndpointDisabled`](crate::UsbError::EndpointDisabled) - The endpoint is not currently
    ///   enabled, due to the device not being configured, or the endpoint belonging to an inactive
    ///   interface alternate setting.
    /// * [`Unsupported`](crate::UsbError::Unsupported) - The endpoint is not a control endpoint.
    pub fn control_read_packet(&mut self, data: &mut [u8]) -> Result<(usize, OutPacketType)> {
        self.core
            .as_mut()
            .ok_or(UsbError::EndpointDisabled)
            .and_then(|c| {
                if c.enabled {
                    c.ep.read_packet(data)
                } else {
                    Err(UsbError::EndpointDisabled)
                }
            })
    }
}

pub struct EndpointIn<U: UsbCore> {
    pub(crate) config: EndpointConfig,
    pub(crate) core: Option<EndpointCore<U::EndpointIn>>,
}

impl<U: UsbCore> EndpointIn<U> {
    pub fn ep_type(&self) -> EndpointType {
        self.config.ep_type
    }

    pub fn max_packet_size(&self) -> u16 {
        self.config.max_packet_size
    }

    pub fn interval(&self) -> u8 {
        self.config.interval
    }

    pub fn address(&self) -> EndpointAddress {
        self.core
            .as_ref()
            .map(|c| c.ep.address())
            .unwrap_or(EndpointAddress(0))
    }

    /// Writes a single packet of data to the specified endpoint. The buffer must not be longer than
    /// the `max_packet_size` specified when allocating the endpoint.
    ///
    /// # Errors
    ///
    /// Note: USB bus implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - The transmission buffer of the USB
    ///   peripheral is full and the packet cannot be sent now. A peripheral may or may not support
    ///   concurrent transmission of packets.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The data is longer than the
    ///   `max_packet_size` specified when allocating the endpoint. This is generally an error in
    ///   the class implementation.
    /// * [`EndpointDisabled`](crate::UsbError::InvalidState) -  The endpoint is currently not
    ///   enabled in the current interface alternate or the device has not been configured by the
    ///   host yet.
    pub fn write_packet(&mut self, data: &[u8]) -> Result<()> {
        self.core
            .as_mut()
            .ok_or(UsbError::EndpointDisabled)
            .and_then(|c| {
                if c.enabled {
                    c.ep.write_packet(data)
                } else {
                    Err(UsbError::EndpointDisabled)
                }
            })
    }
}

/// Specific the type of packet received via [`EndpointOut::control_read_packet`].
#[derive(Debug, Eq, PartialEq)]
pub enum OutPacketType {
    /// A DATA packet
    Data = 0,

    /// A SETUP packet
    Setup = 1,
}

/// USB endpoint address that contains a direction and number.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EndpointAddress(u8);

impl From<u8> for EndpointAddress {
    #[inline]
    fn from(addr: u8) -> EndpointAddress {
        EndpointAddress(addr)
    }
}

impl From<EndpointAddress> for u8 {
    #[inline]
    fn from(addr: EndpointAddress) -> u8 {
        addr.0
    }
}

/*impl From<Option<EndpointAddress>> for u8 {
    #[inline]
    fn from(addr: Option<EndpointAddress>) -> u8 {
        addr.unwrap_or(0)
    }
}*/

impl EndpointAddress {
    const INBITS: u8 = UsbDirection::In as u8;

    /// Constructs a new EndpointAddress with the given number and direction.
    #[inline]
    pub const fn from_parts(number: u8, dir: UsbDirection) -> Self {
        EndpointAddress(number | dir as u8)
    }

    /// Gets the direction part of the address.
    #[inline]
    pub fn direction(&self) -> UsbDirection {
        if (self.0 & Self::INBITS) != 0 {
            UsbDirection::In
        } else {
            UsbDirection::Out
        }
    }

    /// Gets the number part of the endpoint address.
    #[inline]
    pub fn number(&self) -> u8 {
        (self.0 & !Self::INBITS) as u8
    }
}
