use core::cell::Cell;
use endpoint::{Endpoint, EndpointDirection, Direction, EndpointType};
use ::Result;

pub trait UsbBus: Sized {
    fn allocator_state<'a>(&'a self) -> &UsbAllocatorState;
    fn alloc_ep(&self, ep_dir: EndpointDirection, ep_addr: Option<u8>, ep_type: EndpointType,
        max_packet_size: u16, interval: u8) -> Result<u8>;
    fn enable(&self);
    fn reset(&self);
    fn set_device_address(&self, addr: u8);
    fn write(&self, ep_addr: u8, buf: &[u8]) -> Result<usize>;
    fn read(&self, ep_addr: u8, buf: &mut [u8]) -> Result<usize>;
    fn stall(&self, ep_addr: u8);
    fn unstall(&self, ep_addr: u8);
    fn poll(&self) -> PollResult;

    fn allocator<'a>(&'a self) -> UsbAllocator<'a, Self> {
        UsbAllocator(self)
    }
}

pub struct UsbAllocatorState {
    next_interface_number: Cell<u8>,
    next_string_index: Cell<u8>,
}

impl Default for UsbAllocatorState {
    fn default() -> UsbAllocatorState {
        UsbAllocatorState {
            next_interface_number: Cell::new(0),
            // Indices 0-3 are reserved for UsbDevice
            next_string_index: Cell::new(4),
        }
    }
}

pub struct UsbAllocator<'a, B: 'a + UsbBus>(&'a B);

impl<'a, B: UsbBus> UsbAllocator<'a, B> {
    pub fn interface(&self) -> InterfaceNumber {
        let state = self.0.allocator_state();
        let number = state.next_interface_number.get();
        state.next_interface_number.set(number + 1);

        InterfaceNumber(number)
    }

    pub fn string(&self) -> StringIndex {
        let state = self.0.allocator_state();
        let index = state.next_string_index.get();
        state.next_string_index.set(index + 1);

        StringIndex(index)
    }

    pub fn alloc<D: Direction>(&self,
        ep_addr: Option<u8>, ep_type: EndpointType,
        max_packet_size: u16, interval: u8) -> Result<Endpoint<'a, B, D>>
    {
        self.0.alloc_ep(D::DIRECTION, ep_addr, ep_type, max_packet_size, interval)
            .map(|a| Endpoint::new(self.0, a, ep_type, max_packet_size, interval))
    }

    #[inline]
    pub fn control<D: Direction>(&self, max_packet_size: u16) -> Endpoint<'a, B, D> {
        self.alloc(None, EndpointType::Control, max_packet_size, 0).unwrap()
    }

    #[inline]
    pub fn bulk<D: Direction>(&self, max_packet_size: u16) -> Endpoint<'a, B, D> {
        self.alloc(None, EndpointType::Bulk, max_packet_size, 0).unwrap()
    }

    #[inline]
    pub fn interrupt<D: Direction>(&self, max_packet_size: u16, interval: u8)
        -> Endpoint<'a, B, D>
    {
        self.alloc(None, EndpointType::Interrupt, max_packet_size, interval).unwrap()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct InterfaceNumber(u8);

impl From<InterfaceNumber> for u8 {
    fn from(n: InterfaceNumber) -> u8 { n.0 }
}

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

#[derive(Default)]
pub struct PollResult {
    pub reset: bool,
    pub setup: bool,
    pub ep_in_complete: u16,
    pub ep_out: u16,
}