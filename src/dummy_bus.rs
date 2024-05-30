#![allow(unused_variables)]

use crate::bus::UsbBus;

/// Dummy bus implementation with no functionality.
///
/// Examples can create an instance of this bus just to make them compile.
pub struct DummyUsbBus;

impl DummyUsbBus {
    /// Creates a new `DummyUsbBus`.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DummyUsbBus {
    fn default() -> Self {
        Self::new()
    }
}

impl UsbBus for DummyUsbBus {
    fn alloc_ep(
        &mut self,
        ep_dir: crate::UsbDirection,
        ep_addr: Option<crate::class_prelude::EndpointAddress>,
        ep_type: crate::class_prelude::EndpointType,
        max_packet_size: u16,
        interval: u8,
    ) -> crate::Result<crate::class_prelude::EndpointAddress> {
        unimplemented!()
    }

    fn enable(&mut self) {
        unimplemented!()
    }

    fn force_reset(&self) -> crate::Result<()> {
        unimplemented!()
    }

    fn is_stalled(&self, ep_addr: crate::class_prelude::EndpointAddress) -> bool {
        unimplemented!()
    }

    fn poll(&self) -> crate::bus::PollResult {
        unimplemented!()
    }

    fn read(
        &self,
        ep_addr: crate::class_prelude::EndpointAddress,
        buf: &mut [u8],
    ) -> crate::Result<usize> {
        unimplemented!()
    }

    fn reset(&self) {
        unimplemented!()
    }

    fn resume(&self) {
        unimplemented!()
    }

    fn set_device_address(&self, addr: u8) {
        unimplemented!()
    }

    fn set_stalled(&self, ep_addr: crate::class_prelude::EndpointAddress, stalled: bool) {
        unimplemented!()
    }

    fn suspend(&self) {
        unimplemented!()
    }

    fn write(
        &self,
        ep_addr: crate::class_prelude::EndpointAddress,
        buf: &[u8],
    ) -> crate::Result<usize> {
        unimplemented!()
    }
}
