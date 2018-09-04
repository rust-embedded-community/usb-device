use endpoint::{Endpoint, EndpointDirection, Direction, EndpointType};
use utils::FreezableRefCell;
use ::{Result, UsbError};

/// A trait for device-specific USB peripherals. Implement this to add support for a new hardware
/// platform.
///
/// The UsbBus is shared by reference between the global [`UsbDevice`](::device::UsbDevice) as well
/// as [`UsbClass`](::class::UsbClass)es, and therefore any required mutability must be implemented
/// using interior mutability. Most operations that may mutate the bus object itself take place
/// before [`enable`](UsbBus::enable) is called. After the bus is enabled, in practice most access
/// won't mutate the object itself but only endpoint-specific registers and buffers, the access to
/// which is mostly arbitrated by endpoint handles.
pub trait UsbBus: Sized {
    /// Allocates an endpoint and specified endpoint parameters. This method is called by the device
    /// and class implementations to allocate endpoints, and can only be called before
    /// [`UsbBus::enable`] is called.
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
    /// * [`Busy`](::UsbError::Busy) - The bus has already been enabled and no further allocations
    ///   may take place.
    /// * [`EndpointOverflow`](::UsbError::EndpointOverflow) - Available total number of endpoints,
    ///   endpoints of the specified type, or endpoind packet memory has been exhausted. This is
    ///   generally caused when a user tries to add too many classes to a composite device.
    /// * [`EndpointTaken`](::UsbError::EndpointTaken) - A specific `ep_addr` was specified but the
    ///   endpoint in question has already been allocated.
    fn alloc_ep(&mut self, ep_dir: EndpointDirection, ep_addr: Option<u8>, ep_type: EndpointType,
        max_packet_size: u16, interval: u8) -> Result<u8>;

    /// Enables and initializes the USB peripheral. Soon after enabling the device will be reset, so
    /// there is no need to perform a USB reset in this method.
    fn enable(&mut self);

    /// Performs a USB reset. This method should reset the platform-specific peripheral as well as
    /// ensure that all endpoints previously allocate with alloc_ep are initialized as specified.
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
    /// * [`InvalidEndpoint`](::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`Busy`](::UsbError::Busy) - A previously written packet is still pending to be sent.
    ///
    /// Implementations may also return other errors if applicable.
    fn write(&self, ep_addr: u8, buf: &[u8]) -> Result<usize>;

    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet.
    ///
    /// This should also clear any NAK flags and prepare the endpoint to receive the next packet.
    ///
    /// # Errors
    ///
    /// * [`InvalidEndpoint`](::UsbError::InvalidEndpoint) - The `ep_addr` does not point to a
    ///   valid endpoint that was previously allocated with [`UsbBus::alloc_ep`].
    /// * [`NoData`](::UsbError::NoData) - There is no packet to be read. Note that this is
    ///   different from a received zero-length packet, which is valid in USB. A zero-length packet
    ///   will return `Ok(0)`.
    /// * [`BufferOverflow`](::UsbError::BufferOverflow) - The received packet is too long to fix
    ///   in `buf`. This is generally an error in the class implementation.
    ///
    /// Implementations may also return other errors if applicable.
    fn read(&self, ep_addr: u8, buf: &mut [u8]) -> Result<usize>;

    /// Sets the STALL condition for an endpoint.
    fn stall(&self, ep_addr: u8);

    /// Clears the STALL condition of an endpoint. If the endpoint is an OUT endpoint, it should be
    /// prepared to receive data again.
    fn unstall(&self, ep_addr: u8);

    /// Causes the USB peripheral to enter USB suspend mode, lowering power consumption and
    /// preparing to detect a USB wakeup event. This should only be called after
    /// [`poll`](UsbDevice::poll) returns [`PollResult::Suspend]. The device shall stay suspended
    /// using `poll` returns a value other than `Suspend`.
    fn suspend(&self);

    /// Resumes from suspend mode. This may only be called after the peripheral has been previously
    /// suspended.
    fn resume(&self);

    /// Gets information about events and incoming data. See the [`PollResult`] struct for more
    /// information.
    fn poll(&self) -> PollResult;

    /// Simulates a disconnect from the USB bus, causing the host to reset and re-enumerate the
    /// device.
    /// 
    /// Mostly used for development. By calling this at the start of your program ensures that
    /// the host re-enumerates your device after a new program has been flashed.
    /// 
    /// # Errors
    ///
    /// * [`Unsupported`](::UsbError::Unsupported) - This UsbBus implementation doesn't support
    ///   simulating a disconnect or it has not been enabled at creation time.
    fn force_reset(&self) -> Result<()> {
        Err(UsbError::Unsupported)
    }
}

struct WrapperState {
    next_interface_number: u8,
    next_string_index: u8,
}

/// Internal state for [`UsbAllocator`].
///
/// See [`UsbBus::allocator_state`].
pub struct UsbBusWrapper<B: UsbBus> {
    bus: FreezableRefCell<B>,
    state: FreezableRefCell<WrapperState>,
}

impl<B: UsbBus> UsbBusWrapper<B> {
    pub fn new(bus: B) -> UsbBusWrapper<B> {
        UsbBusWrapper {
            bus: FreezableRefCell::new(bus),
            state: FreezableRefCell::new(WrapperState {
                next_interface_number: 0,
                next_string_index: 4,
            }),
        }
    }
}

impl<B: UsbBus> UsbBusWrapper<B> {
    pub fn freeze<'a>(&'a self) -> &B {
        self.bus.borrow_mut().enable();
        self.bus.freeze();
        self.state.freeze();
        self.bus.borrow()
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
    /// This directly delegates to [`UsbBus::alloc_ep`], so see that method for details. This should
    /// rarely be needed by classes.
    pub fn alloc<'a, D: Direction>(&'a self,
        ep_addr: Option<u8>, ep_type: EndpointType,
        max_packet_size: u16, interval: u8) -> Result<Endpoint<'a, B, D>>
    {
        self.bus.borrow_mut().alloc_ep(D::DIRECTION, ep_addr, ep_type, max_packet_size, interval)
            .map(|a| Endpoint::new(&self.bus, a, ep_type, max_packet_size, interval))
    }

    /// Allocates a control endpoint.
    ///
    /// This crate implements the control state machine only for endpoint 0. If classes want to
    /// support control requests in other endpoints, the state machine must be implemented manually.
    /// This should rarely be needed by classes.
    /// /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    #[inline]
    pub fn control<'a, D: Direction>(&'a self, max_packet_size: u16) -> Endpoint<'a, B, D> {
        self.alloc(None, EndpointType::Control, max_packet_size, 0).unwrap()
    }

    /// Allocates a bulk endpoint.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    #[inline]
    pub fn bulk<'a, D: Direction>(&'a self, max_packet_size: u16) -> Endpoint<'a, B, D> {
        self.alloc(None, EndpointType::Bulk, max_packet_size, 0).unwrap()
    }

    /// Allocates an interrupt endpoint.
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Cannot exceed 64 bytes.
    #[inline]
    pub fn interrupt<'a, D: Direction>(&'a self, max_packet_size: u16, interval: u8)
        -> Endpoint<'a, B, D>
    {
        self.alloc(None, EndpointType::Interrupt, max_packet_size, interval).unwrap()
    }
}

/// A handle for a USB interface that contains its number.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct InterfaceNumber(u8);

impl From<InterfaceNumber> for u8 {
    fn from(n: InterfaceNumber) -> u8 { n.0 }
}

/// A handle for a USB string descriptor that contains its index.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct StringIndex(u8);

impl StringIndex {
    pub(crate) fn new(index: u8) -> StringIndex {
        StringIndex(index)
    }
}

impl From<StringIndex> for u8 {
    fn from(i: StringIndex) -> u8 { i.0 }
}

/// Event and incoming packet information returned by [`UsbBus::poll`].
#[derive(Eq, PartialEq)]
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

        /// A SETUP packet has been received. The corresponding bit in `ep_out` may also be set but
        /// is ignored.
        ep_setup: u16
    },

    /// A USB suspend request has been detected or, in the case of self-powered devices, the device
    /// has been disconnected from the USB usb.
    Suspend,

    /// A USB resume request has been detected after being suspended or, in the case of self-powered
    /// devices, the device has been connected to the USB usb.
    Resume,
}