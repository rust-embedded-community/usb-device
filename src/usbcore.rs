use crate::Result;
//use crate::allocator::EndpointConfig;
use crate::endpoint::{EndpointAddress, EndpointConfig, OutPacketType};

/// A trait for accessing device-specific USB peripherals. Implement this to add support for a new
/// hardware peripheral.
pub trait UsbCore: Sized {
    /// The OUT endpoint type for this USB driver.
    type EndpointOut: UsbEndpointOut;

    /// The IN endpoint type for this USB driver.
    type EndpointIn: UsbEndpointIn;

    /// The endpoint allocator type for this USB driver.
    type EndpointAllocator: UsbEndpointAllocator<Self>;

    /// Creates an EndpointAllocator for this UsbCore.
    ///
    /// TODO: This might have to be "take_allocator"
    fn create_allocator(&mut self) -> Self::EndpointAllocator;

    /// Enables and initializes the USB peripheral. `reset` is called soon after enabling the
    /// peripheral, so there's no need to call it yourself.
    fn enable(&mut self, allocator: Self::EndpointAllocator) -> Result<()>;

    /// Handles a USB protocol reset signaled from the host. This method should reset the state of
    /// all endpoints and peripheral flags back to a state compatible with host enumeration, as well
    /// as ensure that all endpoints are disabled.
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

/// Event and incoming packet information returned by [`UsbCore::poll`].
pub enum PollResult {
    /// No events or packets to report.
    None,

    /// The USB reset condition has been detected.
    Reset,

    /// A USB suspend request has been detected or, in the case of self-powered devices, the device
    /// has been disconnected from the USB bus.
    Suspend,

    /// A USB resume request has been detected after being suspended or, in the case of self-powered
    /// devices, the device has been connected to the USB bus.
    Resume,

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
    },
}

/// Allocates endpoint indexes and memory.
pub trait UsbEndpointAllocator<U: UsbCore> {
    /// Allocates an OUT endpoint with the provided configuration
    fn alloc_out(&mut self, config: &EndpointConfig) -> Result<U::EndpointOut>;

    /// Allocates an IN endpoint with the provided configuration
    fn alloc_in(&mut self, config: &EndpointConfig) -> Result<U::EndpointIn>;

    /// Notifies the allocator that a new interface is beginning
    fn begin_interface(&mut self) -> Result<()>;

    /// Notifies the allocator that the next alternate setting for the current interface is
    /// beginning. If the interface has no alternate settings, this is never called.
    fn next_alt_setting(&mut self) -> Result<()>;
}

/// Shared implementation between both OUT and IN endpoints.
pub trait UsbEndpoint {
    /// Gets the address of the endpoint.
    fn address(&self) -> EndpointAddress;

    /// Enables the endpoint with the specified configuration.
    ///
    /// # Safety
    ///
    /// This method is unsafe because enabling two endpoints allocated for different interface
    /// alternate settings simultaneously may result in undefined behavior.
    unsafe fn enable(&mut self, config: &EndpointConfig);

    /// Disables the endpoint.
    fn disable(&mut self);

    /// Gets whether the STALL condition is set for an endpoint.
    fn is_stalled(&self) -> bool;

    /// Sets or clears the STALL condition for the endpoint. If the endpoint is unstalled and it is
    /// an OUT endpoint, it shall be enabled to receive data again.
    fn set_stalled(&mut self, stalled: bool);
}

/// Implementation of an OUT endpoint.
pub trait UsbEndpointOut: UsbEndpoint {
    /// Reads a single packet of data from the specified endpoint and returns the actual length of
    /// the packet and whether it was a DATA or SETUP packet. The buffer must be large enough to fit
    /// at least as many bytes as the `max_packet_size` specified when allocating the endpoint. For
    /// non-control endpoints, the type is always DATA.
    ///
    /// Packets are read in the order they arrive. Peripherals may have a receive buffer that fits
    /// multiple packets per endpoint.
    ///
    /// # Errors
    ///
    /// You may also return other errors as needed.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There is no packet to be read. Note that
    ///   this is different from a received zero-length packet, which is valid and significant in
    ///   USB. A zero-length packet shall return `Ok(0)`.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The received packet is too long to
    ///   fit in `data`.
    fn read_packet(&mut self, data: &mut [u8]) -> Result<(usize, OutPacketType)>;
}

/// Implementation of an IN endpoint.
pub trait UsbEndpointIn: UsbEndpoint {
    /// Writes a single packet of data to the specified endpoint. The buffer must not be longer than
    /// the `max_packet_size` specified when allocating the endpoint.
    ///
    /// # Errors
    ///
    /// You may also return other errors as needed.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - The transmission buffer of the USB
    ///   peripheral is full and the packet cannot be sent now. A peripheral may or may not support
    ///   concurrent transmission of packets.
    /// * [`BufferOverflow`](crate::UsbError::BufferOverflow) - The data is longer than the
    ///   `max_packet_size` specified when allocating the endpoint.
    fn write_packet(&mut self, data: &[u8]) -> Result<()>;

    /// Returns `Ok` if all packets successfully written via `write_packet` have been transmitted to
    /// the host, otherwise an error.
    ///
    /// # Errors
    ///
    /// You may also return other errors as needed.
    ///
    /// * [`WouldBlock`](crate::UsbError::WouldBlock) - There are still untransmitted packets in
    ///   peripheral buffers.
    fn flush(&mut self) -> Result<()>;
}
