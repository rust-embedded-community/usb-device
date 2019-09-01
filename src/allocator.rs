use crate::bus::{UsbBus, EndpointAllocator};
use crate::endpoint::{EndpointAddress, Endpoint, EndpointType};

/// Allocates resources for USB classes.
pub struct UsbAllocator<B: UsbBus> {
    bus: B,
    ep_allocator: B::EndpointAllocator,
    next_interface_number: u8,
    next_string_index: u8,
}

impl<B: UsbBus> UsbAllocator<B> {
    /// Creates a new [`UsbAllocator`] that wraps the provided [`UsbBus`]. Usually only called by
    /// USB driver implementations.
    pub fn new(mut bus: B) -> UsbAllocator<B> {
        UsbAllocator {
            ep_allocator: bus.create_allocator(),
            bus,
            next_interface_number: 0,
            next_string_index: 4,
        }
    }

    /// Allocates a new interface number.
    pub fn interface(&mut self) -> InterfaceNumber {
        let number = self.next_interface_number;
        self.next_interface_number += 1;

        InterfaceNumber(number)
    }

    /// Allocates a new string index.
    pub fn string(&mut self) -> StringIndex {
        let index = self.next_string_index;
        self.next_string_index += 1;

        StringIndex(index)
    }

    /// Allocates an OUT endpoint with the provided configuration
    pub fn endpoint_out(&mut self, config: EndpointConfig) -> B::EndpointOut {
        self.ep_allocator.alloc_out(&config).expect("USB endpoint allocation failed")
    }

    /// Allocates an IN endpoint with the provided configuration
    pub fn endpoint_in(&mut self, config: EndpointConfig) -> B::EndpointIn {
        self.ep_allocator.alloc_in(&config).expect("USB endpoint allocation failed")
    }

    pub(crate) fn finish(self) -> B {
        self.bus
    }
}

/// Configuration for an endpoint allocation.
pub struct EndpointConfig {
    /// The transfer type of the endpoint to be allocated.
    pub ep_type: EndpointType,

    /// Maximum packet size for the endpoint to be allocated.
    pub max_packet_size: u16,

    /// Poll interval for interrupt endpoints.
    pub interval: u8,

    /// Requests a specific endpoint number. Allocation shall fail if the number is not available.
    pub number: Option<u8>,

    /// Specifies that the endpoint is the "pair" of another endpoint.
    ///
    /// If `ep` is an endpoint in the opposite direction, this means that the endpoint to be
    /// allocated uses the same transfer type as `ep` but in the opposite direction in all alternate
    /// settings for the interface.
    ///
    /// If `ep` is an endpoint in the same direction, this means that the two endpoints belong to
    /// different alternate settings for the interface and may never be enabled at the same time.
    pub pair_of: Option<EndpointAddress>,
}

impl EndpointConfig {
    fn new(ep_type: EndpointType, max_packet_size: u16, interval: u8) -> Self {
        EndpointConfig {
            ep_type,
            number: None,
            max_packet_size,
            interval,
            pair_of: None,
        }
    }

    /// Configures a control endpoint.
    ///
    /// This crate implements the control state machine only for endpoint 0. If classes want to
    /// support control requests in other endpoints, the state machine must be implemented manually.
    /// This should rarely be needed by classes.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    ///
    #[inline]
    pub fn control(max_packet_size: u16) -> EndpointConfig {
        Self::new(EndpointType::Control, max_packet_size, 0)
    }

    /// Configures a bulk endpoint.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Must be one of 8, 16, 32 or 64.
    #[inline]
    pub fn bulk(max_packet_size: u16) -> EndpointConfig {
        Self::new(EndpointType::Bulk, max_packet_size, 0)
    }

    /// Configures an interrupt endpoint.
    ///
    /// * `max_packet_size` - Maximum packet size in bytes. Cannot exceed 64 bytes.
    /// * `interval` - The requested polling interval in milliseconds.
    #[inline]
    pub fn interrupt(max_packet_size: u16, interval: u8) -> EndpointConfig {
        Self::new(EndpointType::Interrupt, max_packet_size, interval)
    } 

    /// Requests a specific endpoint number. The endpoint number is the low 4 bits of the endpoint
    /// address. In general this should not be necessary, but it can be used to write devices that
    /// interface with existing host drivers that require a specific endpoint address. The
    /// allocation may fail if the hardware does not support the specified endpoint number or if it
    /// has already been dynamically allocated.
    pub fn number(self, number: u8) -> Self {
        EndpointConfig {
            number: Some(number),
            ..self
        }
    }

    /// Specifies that this endpoint and `ep` belong to the same interface, and:
    ///
    /// If `ep` is an endpoint in the opposite direction, this means that the endpoint to be
    /// allocated uses the same transfer type as `ep` but in the opposite direction in all alternate
    /// settings for the interface.
    ///
    /// If `ep` is an endpoint in the same direction, this means that the two endpoints belong to
    /// different alternate settings for the interface and may never be enabled at the same time.
    ///
    /// Specifying "pairs" for endpoints may enable some implementations to allocate endpoints more
    /// efficiently and they should be specifies whenever possible.
    pub fn pair_of<E: Endpoint>(self, ep: &E) -> Self {
        EndpointConfig {
            pair_of: Some(ep.address()),
            ..self
        }
    }
}

/// A handle for a USB interface that contains its number.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct InterfaceNumber(u8);

impl InterfaceNumber {
    pub(crate) fn new(index: u8) -> InterfaceNumber {
        InterfaceNumber(index)
    }
}

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
