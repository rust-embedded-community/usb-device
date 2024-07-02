//! Experimental device-side USB stack for embedded devices.
//!
//! ## Implementing a USB device
//!
//! A USB device consists of a [`UsbDevice`](device::UsbDevice) instance, one or more
//! [`UsbClass`](crate::class::UsbClass)es, and a platform-specific [`UsbBus`](bus::UsbBus)
//! implementation which together form a USB composite device.
//!
//! In the future USB device implementors will be able to use pre-existing peripheral driver crates
//! and USB class implementation crates. The necessary types for the basic USB composite device
//! implementation are available with:
//!
//! `use usb_device::prelude::*`.
//!
//! See the [`device`] module for a more complete example.
//!
//! ## USB classes
//!
//! For information on how to implement new USB classes, see the [`class`] module and the
//! [`TestClass`](test_class::TestClass) source code for an example of a custom USB device class
//! implementation. The necessary types for creating new classes are available with:
//!
//! `use usb_device::class_prelude::*`.
//!
//! ## USB peripheral drivers
//!
//! New peripheral driver crates can be created by implementing the [`UsbBus`](bus::UsbBus) trait.
//!
//! # Note about terminology
//!
//! This crate uses standard host-centric USB terminology for transfer directions. Therefore an OUT
//! transfer refers to a host-to-device transfer, and an IN transfer refers to a device-to-host
//! transfer. This is mainly a concern for implementing new USB peripheral drivers and USB classes,
//! and people doing that should be familiar with the USB standard.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
mod macros;

/// A USB stack error.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UsbError {
    /// An operation would block because the device is currently busy or there is no data available.
    WouldBlock,

    /// Parsing failed due to invalid input.
    ParseError,

    /// A buffer too short for the data to read was passed, or provided data cannot fit within
    /// length constraints.
    BufferOverflow,

    /// Classes attempted to allocate more endpoints than the peripheral supports.
    EndpointOverflow,

    /// Classes attempted to allocate more packet buffer memory than the peripheral supports. This
    /// can be caused by either a single class trying to allocate a packet buffer larger than the
    /// peripheral supports per endpoint, or multiple allocated endpoints together using more memory
    /// than the peripheral has available for the buffers.
    EndpointMemoryOverflow,

    /// The endpoint address is invalid or already used.
    InvalidEndpoint,

    /// Operation is not supported by device or configuration.
    Unsupported,

    /// Operation is not valid in the current state of the object.
    InvalidState,
}

/// Direction of USB traffic. Note that in the USB standard the direction is always indicated from
/// the perspective of the host, which is backward for devices, but the standard directions are used
/// for consistency.
///
/// The values of the enum also match the direction bit used in endpoint addresses and control
/// request types.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UsbDirection {
    /// Host to device (OUT)
    Out = 0x00,
    /// Device to host (IN)
    In = 0x80,
}

impl From<u8> for UsbDirection {
    fn from(value: u8) -> Self {
        unsafe { core::mem::transmute(value & 0x80) }
    }
}

/// Result for USB operations.
pub type Result<T> = core::result::Result<T, UsbError>;

/// USB control transfers and the SETUP packet.
pub mod control;

/// For implementing peripheral drivers.
pub mod bus;

/// For implementing standard as well as vendor-specific USB classes.
///
/// To implement a new class, implement the [`UsbClass`](class::UsbClass) trait. The trait contains
/// numerous callbacks that you can use to respond to USB events. None of the methods are required,
/// and you only need to override the ones that your specific class needs to function. See the trait
/// documentation for more information on the callback methods.
///
/// Your class should *not* hold a direct reference to the [`UsbBus`](bus::UsbBus) object. Rather it
/// should take a temporary reference to the [`UsbBusAllocator`](bus::UsbBusAllocator) object
/// exposed by the bus in its constructor, and use that to allocate endpoints, as well as interface
/// and string handles. Using the [`Endpoint`](endpoint::Endpoint) handles which wrap a reference to
/// the `UsbBus` instance ensures that classes cannot inadvertently access an endpoint owned by
/// another class.
///
/// In addition to implementing the trait, add struct methods for the end-user to send and receive
/// data via your class. For example, a serial port class might have class-specific methods `read`
/// and `write` to read and write data.
pub mod class;

/// USB endpoints.
pub mod endpoint;

/// USB composite device.
///
/// The [UsbDevice](device::UsbDevice) type in this module is the core of this crate. It combines
/// multiple USB class implementations and the USB bus driver and dispatches bus state changes and
/// control messages between them.
///
/// To implement USB support for your own project, the required code is usually as follows:
///
/// ``` ignore
/// use core::cell::UnsafeCell;
/// use usb_device::prelude::*;
/// use usb_serial; // example class crate (not included)
///
/// static mut CONTROL_BUFFER: UnsafeCell<[u8; 128]> = UnsafeCell::new([0; 128]);
///
/// // Create the device-specific USB peripheral driver. The exact name and arguments are device
/// // specific, so check the documentation for your device driver crate.
/// let usb_bus = device_specific_usb::UsbBus::new(...);
///
/// // Create one or more USB class implementation. The name and arguments depend on the class,
/// // however most classes require the UsbAllocator as the first argument in order to allocate
/// // the required shared resources.
/// let mut serial = usb_serial::SerialPort::new(&usb_bus.allocator());
///
/// // Build the final [UsbDevice](device::UsbDevice) instance. The required arguments are a
/// // reference to the peripheral driver created earlier, as well as a USB vendor ID/product ID
/// // pair. Additional builder arguments can specify parameters such as device class code or
/// // product name. If using an existing class, remember to check the class crate documentation
/// // for correct values.
/// let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x5824, 0x27dd), unsafe { CONTROL_BUFFER.get_mut() })
///     .strings(&[StringDescriptors::new(LangID::EN)
///         .product("Serial port")])
///         .expect("Failed to set strings")
///     .device_class(usb_serial::DEVICE_CLASS)
///     .build().unwrap();
///
/// // At this point the USB peripheral is enabled and a connected host will attempt to enumerate
/// // it.
/// loop {
///     // Must be called more often than once every 10ms to handle events and stay USB compilant,
///     // or from a device-specific interrupt handler.
///     if (usb_dev.poll(&mut [&mut serial])) {
///         // Call class-specific methods here
///         serial.read(...);
///     }
/// }
/// ```
pub mod device;

/// Creating USB descriptors
pub mod descriptor;

pub use descriptor::lang_id::LangID;

/// Test USB class for testing USB driver implementations. Peripheral driver implementations should
/// include an example called "test_class" that creates a device with this class to enable the
/// driver to be tested with the test_class_host example in this crate.
pub mod test_class;

/// Dummy bus with no functionality.
///
/// Examples can create an instance of this bus so they can be compile-checked.
/// Note that the lack of functionality does not allow to run the examples.
///
/// ```
/// use usb_device::dummy::DummyUsbBus;
/// use usb_device::class_prelude::UsbBusAllocator;
///
/// let usb_bus = UsbBusAllocator::new(DummyUsbBus::new());
/// ```
pub mod dummy;

mod control_pipe;

mod device_builder;

/// Prelude for device implementors.
pub mod prelude {
    pub use crate::device::{
        StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbDeviceState, UsbVidPid,
    };
    pub use crate::device_builder::BuilderError;
    pub use crate::LangID;
    pub use crate::UsbError;
}

/// Prelude for class implementors.
pub mod class_prelude {
    pub use crate::bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator};
    pub use crate::class::{ControlIn, ControlOut, UsbClass};
    pub use crate::control;
    pub use crate::descriptor::{BosWriter, DescriptorWriter};
    pub use crate::endpoint::{
        EndpointAddress, EndpointIn, EndpointOut, EndpointType, IsochronousSynchronizationType,
        IsochronousUsageType,
    };
    pub use crate::LangID;
    pub use crate::UsbError;
}

fn _ensure_sync() {
    use crate::bus::PollResult;
    use crate::class_prelude::*;

    struct DummyBus<'a> {
        _a: &'a str,
    }

    impl UsbBus for DummyBus<'_> {
        fn alloc_ep(
            &mut self,
            _ep_dir: UsbDirection,
            _ep_addr: Option<EndpointAddress>,
            _ep_type: EndpointType,
            _max_packet_size: u16,
            _interval: u8,
        ) -> Result<EndpointAddress> {
            Err(UsbError::EndpointOverflow)
        }

        fn enable(&mut self) {}

        fn reset(&self) {}
        fn set_device_address(&self, _addr: u8) {}

        fn write(&self, _ep_addr: EndpointAddress, _buf: &[u8]) -> Result<usize> {
            Err(UsbError::InvalidEndpoint)
        }

        fn read(&self, _ep_addr: EndpointAddress, _buf: &mut [u8]) -> Result<usize> {
            Err(UsbError::InvalidEndpoint)
        }

        fn set_stalled(&self, _ep_addr: EndpointAddress, _stalled: bool) {}
        fn is_stalled(&self, _ep_addr: EndpointAddress) -> bool {
            false
        }
        fn suspend(&self) {}
        fn resume(&self) {}
        fn poll(&self) -> PollResult {
            PollResult::None
        }
    }

    struct DummyClass<'a, B: UsbBus> {
        _ep: crate::endpoint::EndpointIn<'a, B>,
    }

    impl<B: UsbBus> DummyClass<'_, B> {
        fn _new(alloc: &UsbBusAllocator<B>) -> DummyClass<'_, B> {
            DummyClass {
                _ep: alloc.bulk(64),
            }
        }
    }

    impl<B: UsbBus> UsbClass<B> for DummyClass<'_, B> {}

    fn ensure_sync<T: Sync + Send>() {}

    ensure_sync::<DummyBus>();
    ensure_sync::<crate::endpoint::EndpointIn<DummyBus>>();
    ensure_sync::<crate::endpoint::EndpointOut<DummyBus>>();
    ensure_sync::<DummyClass<'_, DummyBus>>();
}
