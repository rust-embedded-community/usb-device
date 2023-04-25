use crate::bus::UsbBus;
use crate::{Result, UsbDirection};
use core::marker::PhantomData;
use portable_atomic::{AtomicPtr, Ordering};

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

/// Isochronous transfers employ one of three synchronization schemes. See USB 2.0 spec 5.12.4.1.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IsochronousSynchronizationType {
    /// Synchronization is not implemented for this endpoint.
    NoSynchronization,
    /// Source and Sink sample clocks are free running.
    Asynchronous,
    /// Source sample clock is locked to Sink, Sink sample clock is locked to data flow.
    Adaptive,
    /// Source and Sink sample clocks are locked to USB SOF.
    Synchronous,
}

/// Intended use of an isochronous endpoint, see USB 2.0 spec sections 5.12 and 9.6.6.
/// Associations between data and feedback endpoints are described in section 9.6.6.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IsochronousUsageType {
    /// Endpoint is used for isochronous data.
    Data,
    /// Feedback for synchronization.
    Feedback,
    /// Endpoint is data and provides implicit feedback for synchronization.
    ImplicitFeedbackData,
}

/// USB endpoint transfer type.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum EndpointType {
    /// Control endpoint. Used for device management. Only the host can initiate requests. Usually
    /// used only endpoint 0.
    Control,
    /// Isochronous endpoint. Used for time-critical unreliable data.
    ///
    /// See USB 2.0 spec section 5.12 "Special Considerations for Isochronous Transfers"
    Isochronous {
        /// Synchronization model used for the data stream that this endpoint relates to.
        synchronization: IsochronousSynchronizationType,
        /// Endpoint's role in the synchronization model selected by [Self::Isochronous::synchronization].
        usage: IsochronousUsageType,
    },
    /// Bulk endpoint. Used for large amounts of best-effort reliable data.
    Bulk,
    /// Interrupt endpoint. Used for small amounts of time-critical reliable data.
    Interrupt,
}

impl EndpointType {
    /// Format EndpointType for use in bmAttributes transfer type field USB 2.0 spec section 9.6.6
    pub fn to_bm_attributes(&self) -> u8 {
        match self {
            EndpointType::Control => 0b00,
            EndpointType::Isochronous {
                synchronization,
                usage,
            } => {
                let sync_bits = match synchronization {
                    IsochronousSynchronizationType::NoSynchronization => 0b00,
                    IsochronousSynchronizationType::Asynchronous => 0b01,
                    IsochronousSynchronizationType::Adaptive => 0b10,
                    IsochronousSynchronizationType::Synchronous => 0b11,
                };
                let usage_bits = match usage {
                    IsochronousUsageType::Data => 0b00,
                    IsochronousUsageType::Feedback => 0b01,
                    IsochronousUsageType::ImplicitFeedbackData => 0b10,
                };
                (usage_bits << 4) | (sync_bits << 2) | 0b01
            }
            EndpointType::Bulk => 0b10,
            EndpointType::Interrupt => 0b11,
        }
    }
}

/// Handle for a USB endpoint. The endpoint direction is constrained by the `D` type argument, which
/// must be either `In` or `Out`.
pub struct Endpoint<'a, B: UsbBus, D: EndpointDirection> {
    bus_ptr: &'a AtomicPtr<B>,
    address: EndpointAddress,
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
    _marker: PhantomData<D>,
}

impl<B: UsbBus, D: EndpointDirection> Endpoint<'_, B, D> {
    pub(crate) fn new(
        bus_ptr: &AtomicPtr<B>,
        address: EndpointAddress,
        ep_type: EndpointType,
        max_packet_size: u16,
        interval: u8,
    ) -> Endpoint<'_, B, D> {
        Endpoint {
            bus_ptr,
            address,
            ep_type,
            max_packet_size,
            interval,
            _marker: PhantomData,
        }
    }

    fn bus(&self) -> &B {
        let bus_ptr = self.bus_ptr.load(Ordering::SeqCst);
        if bus_ptr.is_null() {
            panic!("UsbBus initialization not complete");
        }

        unsafe { &*bus_ptr }
    }

    /// Gets the endpoint address including direction bit.
    pub fn address(&self) -> EndpointAddress {
        self.address
    }

    /// Gets the endpoint transfer type.
    pub fn ep_type(&self) -> EndpointType {
        self.ep_type
    }

    /// Gets the maximum packet size for the endpoint.
    pub fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    /// Gets the poll interval for interrupt endpoints.
    pub fn interval(&self) -> u8 {
        self.interval
    }

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
    /// actually written. The buffer must not be longer than the `max_packet_size` specified when
    /// allocating the endpoint.
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
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        self.bus().write(self.address, data)
    }
}

impl<B: UsbBus> Endpoint<'_, B, Out> {
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
    pub fn read(&self, data: &mut [u8]) -> Result<usize> {
        self.bus().read(self.address, data)
    }
}

/// Type-safe endpoint address.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
