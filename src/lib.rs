//! Device-side USB stack for microcontrollers.
//!
//! This crate contains is used for implementing device-agnostic USB device classes, as well as
//! device-specific USB peripheral drivers.
//!
//! # Users
//!
//! This crate is useful for three distinct groups:
//!
//! ## End-users
//!
//! End-users will often be able to use pre-existing peripheral driver crates and USB class
//! implementation crates. See the [`device`] module for more information. The necessary types for
//! most end-users are conveniently available with `use usb_device::prelude::*`.
//!
//! ## Class implementors
//!
//! For information on how to implement new USB classes, see the [`class`] module. The necessary
//! types for creating new classes are conveniently available with
//! `use usb_device::class_prelude::*`.
//!
//! End-users can also implement new classes if their device uses a proprietary USB based protocol.
//!
//! ## Peripheral driver implementors
//!
//! New peripheral driver crates can be created by implementing the [`bus::UsbBus`] trait.
//!
//! # Note about terminology
//!
//! This crate uses standard host-centric USB terminology for transfer directions. Therefore an OUT
//! transfer refers to a host-to-device transfer, and an IN transfer refers to a device-to-host
//! transfer. This is mainly a concern for implementing new USB peripheral drivers and USB classes,
//! and people doing that should be familiar with the USB standard.

#![no_std]
#![warn(missing_docs)]

/// A USB stack error.
#[derive(Debug)]
pub enum UsbError {
    /// There was no packet available when reading
    NoData,

    /// A previous transfer has not been completed yet
    Busy,

    /// An invalid setup packet was received from the host
    InvalidSetupPacket,

    /// A buffer too short for the received data was passed (fatal)
    BufferOverflow,

    /// Classes attempted to allocate too many endpoints (fatal)
    EndpointOverflow,

    /// Classes attempted to allocate too much packet memory (fatal)
    SizeOverflow,

    /// An invalid endpoint address was used (fatal)
    InvalidEndpoint,

    /// A specific endpoint address has already been allocated (fatal)
    EndpointTaken,
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
/// should take a temporary reference to the [`UsbAllocator`](bus::UsbAllocator) object exposed by
/// the bus in its constructor, and use that to allocate endpoints, as well as interface and string
/// handles. Using the [`Endpoint`](endpoint::Endpoint) handles which wrap a reference to the
/// `UsbBus` instance ensures that classes cannot inadvertently access an endpoint owned by another
/// class.
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
/// use usb_device::prelude::*;
/// use usb_serial; // example class crate (not included)
///
/// // Create the device-specific USB peripheral driver. The exact name and arguments are device
/// // specific, so check the documentation for your device driver crate.
/// let usb_bus = device_specific_usb::UsbBus::new(...);
///
/// // Create one or more USB class implementation. The name and arguments depend on the class,
/// // however most classes require the UsbAllocator as the first argument in order to allocate
/// // the required shared resources.
/// let serial = usb_serial::SerialClass::new(&usb_bus.allocator());
///
/// // Build the final [UsbDevice](device::UsbDevice) instance. The required arguments are a
/// // reference to the peripheral driver created earlier, as well as a USB vendor ID/product ID
/// // pair. Additional builder arguments can specify parameters such as device class code or
/// // product name. If using an existing class, remember to check the class crate documentation
/// // for correct values.
/// let usb_dev = UsbDevice::new(&usb_bus, UsbVidPid(0x5824, 0x27dd))
///     .product("Serial port")
///     .device_class(usb_serial::DEVICE_CLASS)
///     .build(&[&serial]); // pass one or more classes here
///
/// // At this point the USB peripheral is enabled and a connected host will attempt to enumerate
/// // it.
/// loop {
///     // Must be called more often than once every 10ms to handle events and stay USB compilant,
///     // or from a device-specific interrupt handler.
///     usb_dev.poll();
///
///     // Most USB operations are only valid when the device is in the Configured state.
///     if usb_dev.state() == UsbDeviceState::Configured {
///         // Call class-specific methods here
///         serial.read(...);
///     }
/// }
/// ```
pub mod device;

/// Creating USB descriptors
pub mod descriptor;

mod device_builder;
mod device_standard_control;

/// Prelude for end-users.
pub mod prelude {
    pub use ::UsbError;
    pub use ::device::{UsbDevice, UsbDeviceState, UsbDeviceBuilder, UsbVidPid};
}

/// Prelude for class implementors.
pub mod class_prelude {
    pub use ::UsbError;
    pub use ::bus::{UsbBus, UsbAllocator, InterfaceNumber, StringIndex};
    pub use ::device::{ControlOutResult, ControlInResult};
    pub use ::descriptor::DescriptorWriter;
    pub use ::endpoint::{EndpointType, EndpointIn, EndpointOut};
    pub use ::class::UsbClass;
    pub use ::control;
}