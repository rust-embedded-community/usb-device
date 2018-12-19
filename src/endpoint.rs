use core::marker::PhantomData;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::ptr;
use crate::{Result, UsbDirection};
use crate::bus::UsbBus;

/// Trait for endpoint type markers.
pub trait EndpointDirection {
    /// Direction value of the marker type.
    const DIRECTION: UsbDirection;
}

/// Marker type for OUT endpoints.
pub struct Out;

impl EndpointDirection for Out {
    const DIRECTION: UsbDirection = UsbDirection::Out;
}

/// Marker type for IN endpoints.
pub struct In;

impl EndpointDirection for In {
    const DIRECTION: UsbDirection = UsbDirection::In;
}

/// A host-to-device (OUT) endpoint.
pub type EndpointOut<'a, B> = Endpoint<'a, B, Out>;

/// A device-to-host (IN) endpoint.
pub type EndpointIn<'a, B> = Endpoint<'a, B, In>;

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

/// Handle for a USB endpoint. The endpoint direction is constrained by the `D` type argument, which
/// must be either `In` or `Out`.
pub struct Endpoint<'a, B: UsbBus, D: EndpointDirection> {
    bus_ptr: &'a AtomicPtr<B>,
    address: EndpointAddress,
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
    _marker: PhantomData<D>
}

impl<B: UsbBus, D: EndpointDirection> Endpoint<'_, B, D> {
    pub(crate) fn new<'a>(
        bus_ptr: &'a AtomicPtr<B>,
        address: EndpointAddress,
        ep_type: EndpointType,
        max_packet_size: u16,
        interval: u8) -> Endpoint<'_, B, D>
    {
        Endpoint {
            bus_ptr,
            address,
            ep_type,
            max_packet_size,
            interval,
            _marker: PhantomData
        }
    }

    fn bus(&self) -> &B {
        let bus_ptr = self.bus_ptr.load(Ordering::SeqCst);
        if bus_ptr == ptr::null_mut() {
            panic!("UsbBus initialization not complete");
        }

        unsafe { &*bus_ptr }
    }

    /// Gets the endpoint address including direction bit.
    pub fn address(&self) -> EndpointAddress { self.address }

    /// Gets the endpoint transfer type.
    pub fn ep_type(&self) -> EndpointType { self.ep_type }

    /// Gets the maximum packet size for the endpoint.
    pub fn max_packet_size(&self) -> u16 { self.max_packet_size }

    /// Gets the poll interval for interrupt endpoints.
    pub fn interval(&self) -> u8 { self.interval }

    /// Sets the STALL condition for the endpoint.
    pub fn stall(&self) {
        self.bus().set_stalled(self.address, true);
    }

    /// Clears the STALL condition of the endpoint.
    pub fn unstall(&self) {
        self.bus().set_stalled(self.address, false);
    }
}

impl<B: UsbBus> Endpoint<'_, B, In> {
    /// Writes a single packet of data to the specified endpoint and returns number of bytes
    /// actually written.
    ///
    /// The only reason for a short write is if the caller passes a slice larger than the amount of
    /// memory allocated earlier, and this is generally an error in the class implementation.
    ///
    /// # Errors
    ///
    /// Note: USB bus implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`InvalidEndpoint`](crate::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - A previously written packet is still pending
    ///   to be sent.
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        self.bus().write(self.address, data)
    }
}

impl<B: UsbBus> Endpoint<'_, B, Out> {
    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet.
    ///
    /// This should also clear any NAK flags and prepare the endpoint to receive the next packet.
    ///
    /// # Errors
    ///
    /// Note: USB bus implementation errors are directly passed through, so be prepared to handle
    /// other errors as well.
    ///
    /// * [`InvalidEndpoint`](crate::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read. Note that
    ///   this is different from a received zero-length packet, which is valid in USB. A zero-length
    ///   packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fix in `buf`. This is generally an error in the class implementation.
    pub fn read(&self, data: &mut [u8]) -> Result<usize> {
        self.bus().read(self.address, data)
    }
}

/// Type-safe endpoint address.
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

    /// Returns true if the direction is IN, otherwise false.
    #[inline]
    pub fn is_in(&self) -> bool {
        (self.0 & Self::INBITS) != 0
    }

    /// Returns true if the direction is OUT, otherwise false.
    #[inline]
    pub fn is_out(&self) -> bool {
        (self.0 & Self::INBITS) == 0
    }

    /// Gets the index part of the endpoint address.
    #[inline]
    pub fn index(&self) -> usize {
        (self.0 & !Self::INBITS) as usize
    }
}