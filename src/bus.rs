use crate::Result;
use crate::allocator::EndpointConfig;
use crate::endpoint::{EndpointOut, EndpointIn, EndpointAddress};

/// A trait for accessing device-specific USB peripherals. Implement this to add support for a new
/// hardware platform.
pub trait UsbBus: Sized {
    /// The OUT endpoint type for this USB driver.
    type EndpointOut: EndpointOut;

    /// The IN endpoint type for this USB driver.
    type EndpointIn: EndpointIn;

    /// The endpoint allocator type for this USB driver.
    type EndpointAllocator: crate::bus::EndpointAllocator<Self>;

    /// Creates an EndpointAllocator for this UsbBus.
    fn create_allocator(&mut self) -> Self::EndpointAllocator;

    /// Enables and initializes the USB peripheral. Soon after enabling the device will be reset, so
    /// there is no need to perform a USB reset in this method.
    fn enable(&mut self);

    /// Performs a USB reset. This method should reset the platform-specific peripheral as well as
    /// ensure that no endpoints are enabled.
    fn reset(&mut self);

    /// Gets information about events and incoming data. Usually called in a loop or from an
    /// interrupt handler. See the [`PollResult`] struct for more information.
    fn poll(&mut self) -> PollResult;

    /// Sets the device USB address to `addr`.
    fn set_device_address(&mut self, addr: u8);

    /// Sets or clears the STALL condition for an endpoint. If the endpoint is an OUT endpoint, it
    /// will be prepared to receive data again. Only used during control transfers.
    fn set_stalled(&mut self, ep_addr: EndpointAddress, stalled: bool);

    /// Gets whether the STALL condition is set for an endpoint. Only used during control transfers.
    fn is_stalled(&self, ep_addr: EndpointAddress) -> bool;

    /// Causes the USB peripheral to enter USB suspend mode, lowering power consumption and
    /// preparing to detect a USB wakeup event. This will be called after
    /// [`poll`](crate::device::UsbDevice::poll) returns [`PollResult::Suspend`]. The device will
    /// continue be polled, and it shall return a value other than `Suspend` from `poll` when it no
    /// longer detects the suspend condition.
    fn suspend(&mut self);

    /// Resumes from suspend mode. This may only be called after the peripheral has been previously
    /// suspended.
    fn resume(&mut self);
    
    /// Indicates that `set_device_address` must be called before accepting the corresponding
    /// control transfer. 
    ///
    /// The default value for this constant is `false`, which corresponds to the USB 2.0 spec,
    /// 9.4.6. However some platforms take care of delaying the address change in hardware, and
    /// requires the address to be set in advance.
    const QUIRK_SET_ADDRESS_BEFORE_STATUS: bool = false;
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
        ep_setup: u16
    },

    /// A USB suspend request has been detected or, in the case of self-powered devices, the device
    /// has been disconnected from the USB bus.
    Suspend,

    /// A USB resume request has been detected after being suspended or, in the case of self-powered
    /// devices, the device has been connected to the USB bus.
    Resume,
}

/// Allocates endpoint indexes and memory.
pub trait EndpointAllocator<B: UsbBus> {
    /// Allocates an OUT endpoint with the provided configuration
    fn alloc_out(&mut self, config: &EndpointConfig) -> Result<B::EndpointOut>;
    
    /// Allocates an IN endpoint with the provided configuration
    fn alloc_in(&mut self, config: &EndpointConfig) -> Result<B::EndpointIn>;
}