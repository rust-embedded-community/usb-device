use crate::{Result, UsbDirection};

/// Handle for a USB endpoint. The endpoint direction is constrained by the `D` type argument, which
/// must be either `In` or `Out`.
pub trait Endpoint {
    /// Gets the current endpoint address.
    fn address(&self) -> EndpointAddress;

    /// Gets the current endpoint transfer type.
    fn ep_type(&self) -> EndpointType;

    /// Gets the current maximum packet size for the endpoint.
    fn max_packet_size(&self) -> u16;

    /// Gets the current poll interval for interrupt endpoints.
    fn interval(&self) -> u8;

    /// Enables the endpoint with the specified configuration.
    fn enable(&mut self);

    /// Disables the endpoint.
    fn disable(&mut self);

    /// Sets or clears the STALL condition for the endpoint. If the endpoint is an OUT endpoint, it
    /// will be prepared to receive data again.
    fn set_stalled(&mut self, stalled: bool);

    /// Gets whether the STALL condition is set for an endpoint.
    fn is_stalled(&self) -> bool;
}

/// Handle for OUT endpoints.
pub trait EndpointOut: Endpoint {
    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet. The buffer should be large enough to fit at least as many bytes as the
    /// `max_packet_size` specified when allocating the endpoint.
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
    fn read(&mut self, data: &mut [u8]) -> Result<usize>;
}

/// Handle for IN endpoints.
pub trait EndpointIn: Endpoint {
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
    fn write(&mut self, data: &[u8]) -> Result<()>;
}

/// USB endpoint address that contains a direction and index.
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

    /// Constructs a new EndpointAddress with the given index and direction.
    #[inline]
    pub fn from_parts(index: usize, dir: UsbDirection) -> Self {
        EndpointAddress(index as u8 | dir as u8)
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

    /// Gets the index part of the endpoint address.
    #[inline]
    pub fn index(&self) -> usize {
        (self.0 & !Self::INBITS) as usize
    }
}

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
