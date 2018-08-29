use endpoint::{Endpoint, EndpointDirection, Direction, EndpointType};
use ::Result;

pub trait UsbBus: Sized {
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

    fn endpoints<'a>(&'a self) -> EndpointAllocator<'a, Self> {
        EndpointAllocator(self)
    }
}

pub struct EndpointAllocator<'a, B: 'a + UsbBus>(&'a B);

impl<'a, B: UsbBus> EndpointAllocator<'a, B> {
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

#[derive(Default)]
pub struct PollResult {
    pub reset: bool,
    pub setup: bool,
    pub ep_in_complete: u16,
    pub ep_out: u16,
}