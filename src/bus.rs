use crate::endpoint::{
    Endpoint, EndpointAddress, EndpointDirection, EndpointType, IsochronousSynchronizationType,
    IsochronousUsageType,
};
use crate::{Result, UsbDirection, UsbError};
use core::cell::RefCell;
use core::mem;
use core::ptr;
use portable_atomic::{AtomicPtr, Ordering};

/// A trait for device-specific USB peripherals. Implement this to add support for a new hardware
/// platform.
///
/// The UsbBus is shared by reference between the global [`UsbDevice`](crate::device::UsbDevice) as
/// well as [`UsbClass`](crate::class::UsbClass)es, and therefore any required mutability must be
/// implemented using interior mutability. Most operations that may mutate the bus object itself
/// take place before [`enable`](UsbBus::enable) is called. After the bus is enabled, in practice
/// most access won't mutate the object itself but only endpoint-specific registers and buffers, the
/// access to which is mostly arbitrated by endpoint handles.
pub trait UsbBus: Sized {
    /// Allocates an endpoint and specified endpoint parameters. This method is called by the device
    /// and class implementations to allocate endpoints, and can only be called before
    /// [`enable`](UsbBus::enable) is called.
    ///
    /// # Arguments
    ///
    /// * `ep_dir` - The endpoint direction.
    /// * `ep_addr` - A static endpoint address to allocate. If Some, the implementation should
    ///   attempt to return an endpoint with the specified address. If None, the implementation
    ///   should return the next available one.
    /// * `max_packet_size` - Maximum packet size in bytes.
    /// * `interval` - Polling interval parameter for interrupt endpoints.
    ///
    /// # Errors
    ///
    /// * [`EndpointOverflow`](crate::UsbError::EndpointOverflow) - Available total number of
    ///   endpoints, endpoints of the specified type, or endpoind packet memory has been exhausted.
    ///   This is generally caused when a user tries to add too many classes to a composite device.
    /// * [`InvalidEndpoint`](crate::UsbError::InvalidEndpoint) - A specific `ep_addr` was specified
    ///   but the endpoint in question has already been allocated.
    fn alloc_ep(
        &mut self,
        ep_dir: UsbDirection,
        ep_addr: Option<EndpointAddress>,
        ep_type: EndpointType,
        max_packet_size: u16,
        interval: u8,
    ) -> Result<EndpointAddress>;

    /// Enables and initializes the USB peripheral. Soon after enabling the device will be reset, so
    /// there is no need to perform a USB reset in this method.
    fn enable(&mut self);

    /// Called when the host resets the device. This will be soon called after
    /// [`poll`](crate::device::UsbDevice::poll) returns [`PollResult::Reset`]. This method should
    /// reset the state of all endpoints and peripheral flags back to a state suitable for
    /// enumeration, as well as ensure that all endpoints previously allocated with alloc_ep are
    /// initialized as specified.
    fn reset(&self);

    /// Sets the device USB address to `addr`.
    fn set_device_address(&self, addr: u8);

    /// Writes a single packet of data to the specified endpoint and returns number of bytes
    /// actually written.
    ///
    /// The only reason for a short write is if the caller passes a slice larger than the amount of
    /// memory allocated earlier, and this is generally an error in the class implementation.
    ///
    /// # Errors
    ///
    /// * [`InvalidEndpoint`](crate::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - A previously written packet is still pending
    ///   to be sent.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The packet is too long to fit in the
    ///   transmission buffer. This is generally an error in the class implementation, because the
    ///   class shouldn't provide more data than the `max_packet_size` it specified when allocating
    ///   the endpoint.
    ///
    /// Implementations may also return other errors if applicable.
    fn write(&self, ep_addr: EndpointAddress, buf: &[u8]) -> Result<usize>;

    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet.
    ///
    /// This should also clear any NAK flags and prepare the endpoint to receive the next packet.
    ///
    /// # Errors
    ///
    /// * [`InvalidEndpoint`](crate::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read. Note that
    ///   this is different from a received zero-length packet, which is valid in USB. A zero-length
    ///   packet will return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `buf`. This is generally an error in the class implementation, because the class
    ///   should use a buffer that is large enough for the `max_packet_size` it specified when
    ///   allocating the endpoint.
    ///
    /// Implementations may also return other errors if applicable.
    fn read(&self, ep_addr: EndpointAddress, buf: &mut [u8]) -> Result<usize>;

    /// Sets or clears the STALL condition for an endpoint. If the endpoint is an OUT endpoint, it
    /// should be prepared to receive data again.
    fn set_stalled(&self, ep_addr: EndpointAddress, stalled: bool);

    /// Gets whether the STALL condition is set for an endpoint.
    fn is_stalled(&self, ep_addr: EndpointAddress) -> bool;

    /// Causes the USB peripheral to enter USB suspend mode, lowering power consumption and
    /// preparing to detect a USB wakeup event. This will be called after
    /// [`poll`](crate::device::UsbDevice::poll) returns [`PollResult::Suspend`]. The device will
    /// continue be polled, and it shall return a value other than `Suspend` from `poll` when it no
    /// longer detects the suspend condition.
    fn suspend(&self);

    /// Resumes from suspend mode. This may only be called after the peripheral has been previously
    /// suspended.
    fn resume(&self);

    /// Gets information about events and incoming data. Usually called in a loop or from an
    /// interrupt handler. See the [`PollResult`] struct for more information.
    fn poll(&self) -> PollResult;

    /// Simulates a disconnect from the USB bus, causing the host to reset and re-enumerate the
    /// device.
    ///
    /// The default implementation just returns `Unsupported`.
    ///
    /// # Errors
    ///
    /// * [`Unsupported`](crate::UsbError::Unsupported) - This UsbBus implementation doesn't support
    ///   simulating a disconnect or it has not been enabled at creation time.
    fn force_reset(&self) -> Result<()> {
        Err(UsbError::Unsupported)
    }

    /// Indicates that `set_device_address` must be called before accepting the corresponding
    /// control transfer, not after.
    ///
    /// The default value for this constant is `false`, which corresponds to the USB 2.0 spec, 9.4.6
    const QUIRK_SET_ADDRESS_BEFORE_STATUS: bool = false;
}

struct AllocatorState {
    next_interface_number: u8,
    next_string_index: u8,
}

/// Helper type used for UsbBus resource allocation and initialization.
pub struct UsbBusAllocator<B: UsbBus> {
    bus: RefCell<B>,
    bus_ptr: AtomicPtr<B>,
    state: RefCell<AllocatorState>,
}

impl<B: UsbBus> UsbBusAllocator<B> {
    /// Creates a new [`UsbBusAllocator`] that wraps the provided [`UsbBus`]. Usually only called by
    /// USB driver implementations.
    pub fn new(bus: B) -> UsbBusAllocator<B> {
        UsbBusAllocator {
            bus: RefCell::new(bus),
            bus_ptr: AtomicPtr::new(ptr::null_mut()),
            state: RefCell::new(AllocatorState {
                next_interface_number: 0,
                next_string_index: 4,
            }),
        }
    }

    pub(crate) fn freeze(&self) -> &B {
        // Prevent further allocation by borrowing the allocation state permanently.
        mem::forget(self.state.borrow_mut());

        // Enable the USB bus
        self.bus.borrow_mut().enable();

        // An AtomicPtr is used for the reference from Endpoints to UsbBus, in order to ensure that
        // Endpoints stay Sync (if the Endpoints had a reference to a RefCell, they would not be
        // Sync) Set the pointer used by the Endpoints to access the UsbBus to point to the UsbBus
        // in the RefCell.
        let mut bus_ref = self.bus.borrow_mut();
        let bus_ptr_v = &mut *bus_ref as *mut B;
        self.bus_ptr.store(bus_ptr_v, Ordering::SeqCst);

        // And then leave the RefCell borrowed permanently so that it cannot be borrowed mutably
        // anymore.
        mem::forget(bus_ref);

        // Return the reference to the UsbBus, for use by UsbDevice.
        unsafe { &*bus_ptr_v }
    }

    /// Allocates a new interface number.
    pub fn interface(&self) -> InterfaceNumber {
        let mut state = self.state.borrow_mut();
        let number = state.next_interface_number;
        state.next_interface_number += 1;

        InterfaceNumber(number)
    }

    /// Allocates a new string index.
    pub fn string(&self) -> StringIndex {
        let mut state = self.state.borrow_mut();
        let index = state.next_string_index;
        state.next_string_index += 1;

        StringIndex(index)
    }

    /// Allocates an endpoint with the specified direction and address.
    ///
    /// This directly delegates to [`UsbBus::alloc_ep`], so see that method for details. In most
    /// cases classes should call the endpoint type specific methods instead.
    pub fn alloc<D: EndpointDirection>(
        &self,
        ep_addr: Option<EndpointAddress>,
        ep_type: EndpointType,
        max_packet_size: u16,
        interval: u8,
    ) -> Result<Endpoint<'_, B, D>> {
        self.bus
            .borrow_mut()
            .alloc_ep(D::DIRECTION, ep_addr, ep_type, max_packet_size, interval)
            .map(|a| Endpoint::new(&self.bus_ptr, a, ep_type, max_packet_size, interval))
    }

    /// Allocates a control endpoint.
    ///
    /// This crate implements the control state machine only for endpoint 0. If classes want to
    /// support control requests in other endpoints, the state machine must be implemented manually.
    /// This should rarely be needed by classes.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    ///
    /// # Panics
    ///
    /// Panics if endpoint allocation fails, because running out of endpoints or memory is not
    /// feasibly recoverable.
    #[inline]
    pub fn control<D: EndpointDirection>(&self, max_packet_size: u16) -> Endpoint<'_, B, D> {
        self.alloc(None, EndpointType::Control, max_packet_size, 0)
            .expect("alloc_ep failed")
    }

    /// Allocates an isochronous endpoint.
    ///
    /// # Arguments
    ///
    /// * `synchronization` - Type of synchronization used by the endpoint
    /// * `usage` - Whether the endpoint is data, explicit feedback, or data+implicit feedback
    /// * `payload_size` - Payload size in bytes.
    /// * `interval` - Interval for polling, expressed in frames/microframes.
    ///
    /// See USB 2.0 section 9.6.6.
    ///
    /// # Panics
    ///
    /// Panics if endpoint allocation fails, because running out of endpoints or memory is not
    /// feasibly recoverable.
    #[inline]
    pub fn isochronous<D: EndpointDirection>(
        &self,
        synchronization: IsochronousSynchronizationType,
        usage: IsochronousUsageType,
        payload_size: u16,
        interval: u8,
    ) -> Endpoint<'_, B, D> {
        self.alloc(
            None,
            EndpointType::Isochronous {
                synchronization,
                usage,
            },
            payload_size,
            interval,
        )
        .expect("alloc_ep failed")
    }

    /// Allocates a bulk endpoint.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    ///
    /// # Panics
    ///
    /// Panics if endpoint allocation fails, because running out of endpoints or memory is not
    /// feasibly recoverable.
    #[inline]
    pub fn bulk<D: EndpointDirection>(&self, max_packet_size: u16) -> Endpoint<'_, B, D> {
        self.alloc(None, EndpointType::Bulk, max_packet_size, 0)
            .expect("alloc_ep failed")
    }

    /// Allocates an interrupt endpoint.
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Cannot exceed 64 bytes.
    /// * `interval` - Polling interval.
    ///
    /// # Panics
    ///
    /// Panics if endpoint allocation fails, because running out of endpoints or memory is not
    /// feasibly recoverable.
    #[inline]
    pub fn interrupt<D: EndpointDirection>(
        &self,
        max_packet_size: u16,
        interval: u8,
    ) -> Endpoint<'_, B, D> {
        self.alloc(None, EndpointType::Interrupt, max_packet_size, interval)
            .expect("alloc_ep failed")
    }
}

/// A handle for a USB interface that contains its number.
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InterfaceNumber(pub(crate) u8);

impl From<InterfaceNumber> for u8 {
    fn from(n: InterfaceNumber) -> u8 {
        n.0
    }
}

/// A handle for a USB string descriptor that contains its index.
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct StringIndex(u8);

impl StringIndex {
    pub(crate) fn new(index: u8) -> StringIndex {
        StringIndex(index)
    }
}

impl From<StringIndex> for u8 {
    fn from(i: StringIndex) -> u8 {
        i.0
    }
}

/// Event and incoming packet information returned by [`UsbBus::poll`].
pub enum PollResult {
    /// No events or packets to report.
    None,

    /// The USB reset condition has been detected.
    Reset,

    /// USB packets have been received or sent. Each data field is a bit-field where the least
    /// significant bit represents endpoint 0 etc., and a set bit signifies the event has occurred
    /// for the corresponding endpoint.
    Data {
        /// An OUT packet has been received. This event should continue to be reported until the
        /// packet is read.
        ep_out: u16,

        /// An IN packet has finished transmitting. This event should only be reported once for each
        /// completed transfer.
        ep_in_complete: u16,

        /// A SETUP packet has been received. This event should continue to be reported until the
        /// packet is read. The corresponding bit in `ep_out` may also be set but is ignored.
        ep_setup: u16,
    },

    /// A USB suspend request has been detected or, in the case of self-powered devices, the device
    /// has been disconnected from the USB bus.
    Suspend,

    /// A USB resume request has been detected after being suspended or, in the case of self-powered
    /// devices, the device has been connected to the USB bus.
    Resume,
}
