use ::Result;

pub trait UsbBus {
    fn enable(&self);
    fn reset(&self);
    fn configure_ep(&self, ep_addr: u8, ep_type: EndpointType, max_packet_size: u16) -> Result<()>;
    fn set_device_address(&self, addr: u8);
    fn write(&self, ep_addr: u8, buf: &[u8]) -> Result<usize>;
    fn read(&self, ep_addr: u8, buf: &mut [u8]) -> Result<usize>;
    fn stall(&self, ep_addr: u8);
    fn unstall(&self, ep_addr: u8);
    fn poll(&self) -> PollResult;
}

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum EndpointType {
    Control = 0b00,
    Isochronous = 0b01,
    Bulk = 0b10,
    Interrupt = 0b11,
}

// bEndpointAddress:
// D7: Direction 0 = OUT, 1 = IN

pub struct EndpointPair<'a, B: 'a + UsbBus> {
    bus: &'a B,
    address: u8,
}

impl<'a, B: UsbBus> EndpointPair<'a, B> {
    pub fn new(bus: &'a B, address: u8) -> EndpointPair<'a, B> {
        EndpointPair { bus, address }
    }

    pub fn split(self, ep_type: EndpointType, max_packet_size: u16) -> (EndpointOut<'a, B>, EndpointIn<'a, B>) {
        let ep_out = EndpointOut {
            bus: self.bus,
            address: self.address,
            ep_type: ep_type,
            max_packet_size,
            interval: 1,
        };

        let ep_in = EndpointIn {
            bus: self.bus,
            address: self.address | 0x80,
            ep_type: ep_type,
            max_packet_size,
            interval: 1,
        };

        (ep_out, ep_in)
    }
}

pub trait Endpoint {
    fn address(&self) -> u8;
    fn ep_type(&self) -> EndpointType;
    fn max_packet_size(&self) -> u16;
    fn interval(&self) -> u8;
}

pub struct EndpointOut<'a, B: 'a + UsbBus> {
    bus: &'a B,
    address: u8,
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
}

impl<'a, B: UsbBus> EndpointOut<'a, B> {
    pub fn configure(&self) -> Result<()> {
        self.bus.configure_ep(self.address, self.ep_type, self.max_packet_size)
    }

    pub fn read(&self, data: &mut [u8]) -> Result<usize> {
        self.bus.read(self.address, data)
    }

    pub fn stall(&self) {
        self.bus.stall(self.address);
    }

    pub fn unstall(&self) {
        self.bus.unstall(self.address);
    }
}

impl<'a, B: UsbBus> Endpoint for EndpointOut<'a, B> {
    fn address(&self) -> u8 { self.address }
    fn ep_type(&self) -> EndpointType { self.ep_type }
    fn max_packet_size(&self) -> u16 { self.max_packet_size }
    fn interval(&self) -> u8 { self.interval }
}

pub struct EndpointIn<'a, B: 'a + UsbBus> {
    bus: &'a B,
    address: u8,
    ep_type: EndpointType,
    max_packet_size: u16,
    interval: u8,
}

impl<'a, B: UsbBus> EndpointIn<'a, B> {
    pub fn configure(&self) -> Result<()> {
        self.bus.configure_ep(self.address, self.ep_type, self.max_packet_size)
    }

    pub fn write(&self, data: &[u8]) -> Result<usize> {
        self.bus.write(self.address, data)
    }

    pub fn stall(&self) {
        self.bus.stall(self.address);
    }

    pub fn unstall(&self) {
        self.bus.unstall(self.address);
    }
}

impl<'a, B: UsbBus> Endpoint for EndpointIn<'a, B> {
    fn address(&self) -> u8 { self.address }
    fn ep_type(&self) -> EndpointType { self.ep_type }
    fn max_packet_size(&self) -> u16 { self.max_packet_size }
    fn interval(&self) -> u8 { self.interval }
}

#[derive(Default)]
pub struct PollResult {
    pub reset: bool,
    pub setup: bool,
    pub ep_in_complete: u16,
    pub ep_out: u16,
}