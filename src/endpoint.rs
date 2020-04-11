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

/// Configuration and descriptor information for a USB endpoint.
pub struct EndpointConfig {
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
    fixed_address: Option<EndpointAddress>,
}

impl EndpointConfig {
    /// Creates configuration for a control endpoint with the maximum specified packet size in
    /// bytes. This method should almost never be needed by classes, because control transfers are
    /// automatically handled by usb-device.
    pub const fn control(max_packet_size: u16) -> Self {
        Self {
            ep_type: EndpointType::Control,
            max_packet_size,
            interval: 0,
            fixed_address: None,
        }
    }

    /// Creates configuration for a bulk endpoint with the specified maximum packet size in bytes.
    pub const fn bulk(max_packet_size: u16) -> Self {
        Self {
            ep_type: EndpointType::Bulk,
            max_packet_size,
            interval: 0,
            fixed_address: None,
        }
    }

    /// Creates configuration for an interrupt endpoint with the specified maximum packet size in
    /// bytes and poll interval in milliseconds. The minimum interval is 1.
    pub const fn interrupt(max_packet_size: u16, interval: u8) -> Self {
        Self {
            ep_type: EndpointType::Interrupt,
            max_packet_size,
            interval,
            fixed_address: None,
        }
    }

    /// Specifies that the endpoints must have a fixed address. This can be used to create classes
    /// that are compatible with existing drivers that expect certain endpoint addresses. Otherwise
    /// you should leave this unspecified in order allow for drivers to allocate endpoints as
    /// efficiently as possible. You must also ensure that the address is for the correct direction.
    ///
    /// Drivers may have arbitrary restrictions on the available endpoint configurations including
    /// which addresses are supported and what transfer types are supported for each address. If the
    /// driver does not support a fixed address configuration, `EndpointUnavailable` will be
    /// returned in the allocation phase.
    ///
    /// This is also used internally to allocate control endpoint 0.
    pub const fn with_fixed_address(self, address: EndpointAddress) -> Self {
        Self {
            fixed_address: Some(address),
            ..self
        }
    }

    /// Returns the endpoint type.
    pub const fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    /// Returns the maximum packet size in bytes.
    pub const fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    /// Returns the poll interval for the endpoint in milliseconds. Ignored for control and bulk
    /// endpoints.
    pub const fn interval(&self) -> u8 {
        self.interval
    }

    /// Returns the fixed endpoint address, if specified.
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

/// USB OUT (host-to-device) endpoint.
pub struct EndpointOut<U: UsbCore> {
    pub(crate) config: EndpointConfig,
    pub(crate) core: Option<EndpointCore<U::EndpointOut>>,
}

impl<U: UsbCore> EndpointOut<U> {
    /// Returns the endpoint configuration.
    pub fn config(&self) -> &EndpointConfig {
        &self.config
    }

    pub(crate) fn address_option(&self) -> Option<EndpointAddress> {
        self.core
            .as_ref()
            .map(|c| c.ep.address())
    }

    /// Returns the endpoint's address. If the address hasn't been allocated yet, returns a dummy
    /// address that is never valid.
    pub fn address(&self) -> EndpointAddress {
        self.address_option().unwrap_or(EndpointAddress(0xff))
    }

    /// Returns whether this endpoint is currently enabled. For an endpoint to be enabled, it must
    /// have been allocated, the device must be in the Configured state, and the endpoint must be in
    /// a currently enabled interface alternate setting.
    pub fn is_enabled(&self) -> bool {
        self.core.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet. The buffer must be large enough to fit at least as many bytes as the
    /// `max_packet_size` specified when allocating the endpoint.
    ///
    /// Packets are read in the order they arrive. Peripheral implementations may have a buffer that
    /// fits multiple packets per endpoint, so it's possible that calling this function in a loop
    /// may succeed more than once. Packets in buffers may be discarded if the host issues a reset
    /// or changes alternate settings while there is still data waiting to be sent.
    ///
    /// # Errors
    ///
    /// Note: UsbCore implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read or the
    ///   endpoint is disabled. Note that this is different from a received zero-length packet,
    ///   which is valid and significant in USB. A zero-length packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `data`. This is generally an error in the class implementation.
    pub fn read_packet(&mut self, data: &mut [u8]) -> Result<usize> {
        self.control_read_packet(data).map(|(count, _)| count)
    }

    /// The same as [`read_packet`](EndpointOut::read_packet), but also returns whether the packet
    /// was a DATA or SETUP packet. This method should almost never be needed by classes, because
    /// control transfers are automatically handled by usb-device.
    ///
    /// This method exists only so that a class can be made compatible with a weirdly implemented
    /// existing device that does control transfers on an endpoint other than EP0. Not all USB
    /// hardware supports control transfers on arbitrary endpoints, in which case allocating such an
    /// endpoint will fail.
    ///
    /// # Errors
    ///
    /// Note: UsbCore implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read or the
    ///   endpoint is disabled. Note that this is different from a received zero-length packet,
    ///   which is valid and significant in USB. A zero-length packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `data`. This is generally an error in the class implementation.
    pub fn control_read_packet(&mut self, data: &mut [u8]) -> Result<(usize, OutPacketType)> {
        self.core
            .as_mut()
            .ok_or(UsbError::WouldBlock)
            .and_then(|c| {
                if c.enabled {
                    c.ep.read_packet(data)
                } else {
                    Err(UsbError::WouldBlock)
                }
            })
    }
}

/// USB OUT (host-to-device) endpoint.
pub struct EndpointIn<U: UsbCore> {
    pub(crate) config: EndpointConfig,
    pub(crate) core: Option<EndpointCore<U::EndpointIn>>,
}

impl<U: UsbCore> EndpointIn<U> {
    /// Returns the endpoint configuration.
    pub fn config(&self) -> &EndpointConfig {
        &self.config
    }

    pub(crate) fn address_option(&self) -> Option<EndpointAddress> {
        self.core
            .as_ref()
            .map(|c| c.ep.address())
    }

    /// Returns the endpoint's address. If the address hasn't been allocated yet, returns a dummy
    /// address that is never valid.
    pub fn address(&self) -> EndpointAddress {
        self.address_option().unwrap_or(EndpointAddress(0xff))
    }

    /// Returns whether this endpoint is currently enabled. For an endpoint to be enabled, it must
    /// have been allocated, the device must be in the Configured state, and the endpoint must be in
    /// a currently enabled interface alternate setting.
    pub fn is_enabled(&self) -> bool {
        self.core.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    /// Writes a single packet of data to the specified endpoint. The buffer must not be longer than
    /// the `max_packet_size` specified when allocating the endpoint.
    ///
    /// Packets are sent in the order they are written. Peripheral implementations may have a buffer
    /// that fits multiple packets per endpoint, so it's possible that calling this function in a
    /// loop may succeed more than once. Packets in buffers may be discarded if the host issues a
    /// reset or changes alternate settings while there is still data waiting to be sent.
    ///
    /// # Errors
    ///
    /// Note: UsbCore implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - The transmission buffer of the USB
    ///   peripheral is full and the packet cannot be sent now, or the endpoint is disabled. A
    ///   peripheral may or may not support concurrent transmission of packets.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The data is longer than the
    ///   `max_packet_size` specified when allocating the endpoint. This is generally an error in
    ///   the class implementation.
    pub fn write_packet(&mut self, data: &[u8]) -> Result<()> {
        self.core
            .as_mut()
            .ok_or(UsbError::WouldBlock)
            .and_then(|c| {
                if c.enabled {
                    c.ep.write_packet(data)
                } else {
                    Err(UsbError::WouldBlock)
                }
            })
    }

    /// Returns `Ok` if all packets successfully written via `write_packet` have been transmitted to
    /// the host, otherwise an error.
    ///
    /// # Errors
    ///
    /// Note: UsbCore implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There are still untransmitted packets in
    ///   peripheral buffers.
    pub fn flush(&mut self) -> Result<()> {
        match self.core.as_mut() {
            Some(c) => {
                if c.enabled {
                    c.ep.flush()
                } else {
                    Ok(())
                }
            }
            None => Ok(())
        }
    }
}

/// Type of packet received via [`EndpointOut::control_read_packet`].
#[derive(Debug, Eq, PartialEq)]
pub enum OutPacketType {
    /// DATA packet
    Data = 0,

    /// SETUP packet
    Setup = 1,
}

/// USB endpoint address that contains a direction and number.
///
/// Unallocated endpoints will return dummy addresses that aren't a valid address for an endpoint.
/// Dummy addresses are used so that in the allocation phase descriptor generation will not result
/// in an error, even though the generated descriptor is invalid.
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
