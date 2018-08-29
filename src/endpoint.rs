use core::marker::PhantomData;
use ::Result;
use bus::UsbBus;

pub trait Direction {
    const DIRECTION: EndpointDirection;
}

pub struct Out;
impl Direction for Out {
    const DIRECTION: EndpointDirection = EndpointDirection::Out;
}

pub struct In;
impl Direction for In {
    const DIRECTION: EndpointDirection = EndpointDirection::In;
}

pub type EndpointOut<'a, B> = Endpoint<'a, B, Out>;
pub type EndpointIn<'a, B> = Endpoint<'a, B, In>;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EndpointDirection {
    Out = 0x00,
    In = 0x80,
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EndpointType {
    Control = 0b00,
    Isochronous = 0b01,
    Bulk = 0b10,
    Interrupt = 0b11,
}

pub struct Endpoint<'a, B: 'a + UsbBus, D: Direction> {
    bus: &'a B,
    address: u8,
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
    _marker: PhantomData<D>
}

impl<'a, B: UsbBus, D: Direction> Endpoint<'a, B, D> {
    pub(crate) fn new(bus: &'a B, address: u8, ep_type: EndpointType,
        max_packet_size: u16, interval: u8) -> Endpoint<'a, B, D>
    {
        Endpoint {
            bus,
            address,
            ep_type,
            max_packet_size,
            interval,
            _marker: PhantomData
        }
    }

    pub fn address(&self) -> u8 { self.address }
    pub fn ep_type(&self) -> EndpointType { self.ep_type }
    pub fn max_packet_size(&self) -> u16 { self.max_packet_size }
    pub fn interval(&self) -> u8 { self.interval }

    pub fn stall(&self) {
        self.bus.stall(self.address);
    }

    pub fn unstall(&self) {
        self.bus.unstall(self.address);
    }
}

impl<'a, B: UsbBus> Endpoint<'a, B, Out> {
    pub fn read(&self, data: &mut [u8]) -> Result<usize> {
        self.bus.read(self.address, data)
    }
}

impl<'a, B: UsbBus> Endpoint<'a, B, In> {
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        self.bus.write(self.address, data)
    }
}
