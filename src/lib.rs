//! Experimental device-side USB stack for embedded devices.
//!
//! ## Implementing a USB device
//!
//! A USB device consists of a [`UsbDevice`](device::UsbDevice) instance, one or more
//! [`UsbClass`](crate::class::UsbClass)es, and a platform-specific [`UsbCore`](usbcore::UsbCore)
//! implementation which together form a USB device. The crate is able to automatically allocate
//! resources to and coordinate multiple separate classes in the same device, so that the same
//! device can appear as e.g. a keyboard as well as a serial port at the same time.
//!
//! Some USB class crates are already available, so you might not have to implement a USB class by
//! hand for your application. Check [usb-device on
//! crates.io](https://crates.io/search?q=usb-device) before writing your own! Device drivers are
//! also available under the same keyword, unless already built into your HAL crate.
//!
//! # Example
//!
//! This is an example of a USB application. See [`class`] for an example on how to implement new
//! USB classes.
//!
//! ```ignore
//! use usb_device::prelude::*; // the prelude is geared towards creating USB applications
//! use usb_device::test_class::TestClass; // using the built-in test class as a sample
//!
//! // Create the USB peripheral driver. The exact name and arguments are platform specific, so
//! // check the documentation for your device driver crate.
//! let usb = device_specific_usb::UsbCore::new();
//!
//! // Create one or more USB classes.
//! let mut usb_class = TestClass::new();
//!
//! // Build the final UsbDevice instance. The required arguments are the peripheral driver
//! // created earlier, as well as a USB vendor ID/product ID pair. Additional builder arguments
//! // can specify parameters such as device class code or product name. If using an existing
//! // class,remember to check the class crate documentation for correct values for fields such
//! // as device_class.
//! let mut usb_dev = UsbDeviceBuilder::new(usb, UsbVidPid(0x5824, 0x27dd))
//!     .device_class(0xff) // vendor specific
//!     .product("My product")
//!     .build(&mut usb_class) // for multiple classes: .poll((&mut c1, &mut c2))
//!     .expect("device creation failed");
//!
//! // At this point the USB peripheral is enabled and a the host will attempt to enumerate it if
//! // it's plugged in.
//! loop {
//!     // Must be called more often than once every 10ms to handle events and stay USB compliant,
//!     // or from a device-specific interrupt handler. The list of classes must be the same
//!     // classes in the same order as at device creation time.
//!     if usb_dev.poll(&mut usb_class).is_ok() {
//!         // Call class-specific methods here
//!     }
//! }
//! ```
//!
//! ## USB classes
//!
//! For information on how to implement new USB classes, see the [`class`] module and the
//! [`TestClass`](test_class::TestClass) source code for an example of a custom USB device class
//! implementation. The necessary types for creating new classes are available with:
//!
//! `use usb_device::class::*`.
//!
//! ## USB peripheral drivers
//!
//! New peripheral driver crates can be created by implementing the [`UsbCore`](usbcore::UsbCore)
//! trait.
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
    /// An operation would block because the device is currently busy or there is no data available.
    WouldBlock,

    /// Parsing failed due to invalid input.
    ParseError,

    /// A buffer too short for the data to read was passed, or provided data cannot fit within
    /// length constraints.
    BufferOverflow,

    /// A fixed address endpoint allocation failed either because two classes attempted to allocate
    /// the same address, or the hardware does not support the endpoint address and configuration
    /// combination.
    EndpointUnavailable,

    /// Classes attempted to allocate more endpoints than the peripheral supports.
    EndpointOverflow,

    /// Classes attempted to allocate more packet buffer memory than the peripheral supports. This
    /// can be caused by either a single class trying to allocate a packet buffer larger than the
    /// peripheral supports per endpoint, or multiple allocated endpoints together using more memory
    /// than the peripheral has available for the buffers.
    EndpointMemoryOverflow,

    /// The endpoint address is invalid or already used.
    InvalidEndpoint,

    /// The interface requested does not exist.
    InvalidInterface,

    /// The alternate setting requested for the interface does not exist.
    InvalidAlternateSetting,

    /// Operation is not supported by device or configuration.
    Unsupported,

    /// Operation is not valid in the current state of the object.
    InvalidState,

    /// The object was attempted to be configured twice.
    DuplicateConfig,

    /// An unknown platform-specific error has occurred.
    Platform,

    /// Early return from some configuration operations. Not really an error.
    #[doc(hidden)]
    Break,
}

/// Direction of USB traffic. Note that in the USB standard the direction is always indicated from
/// the perspective of the host, which is backward for devices, but the standard directions are used
/// for consistency.
///
/// The values of the enum also match the direction bit used in endpoint addresses and control
/// request types.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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

/// USB control transfers and the SETUP packet requests.
pub mod control;

/// Traits that must be implemented by peripheral drivers.
pub mod usbcore;

/// Traits and objects used for implementing standard as well as vendor-specific USB classes.
///
/// To implement a new class, implement the [`UsbClass`](class::UsbClass) trait. The trait contains
/// numerous callbacks that you can use to respond to USB events. See the trait documentation for
/// more information on the callback methods.
///
/// In addition to implementing the trait, add struct methods for the end-user to send and receive
/// data via your class. For example, a serial port class might have class-specific `read` and
/// `write` methods.
pub mod class;

/// USB endpoints.
pub mod endpoint;

/// USB composite device.
///
/// The [UsbDevice](device::UsbDevice) type in this module is the core of this crate. It combines
/// multiple USB class implementations and the USB driver and dispatches state changes and control
/// messages between them.
pub mod device;

/// Writers for creating USB descriptors. Can also be usedto
pub mod descriptor;

/// Test USB class for testing USB driver implementations. Peripheral driver implementations should
/// include an example called "test_class" that creates a device with this class to enable the
/// driver to be tested with the test_class_host example in this crate.
pub mod test_class;

mod allocator;
mod class_list;
mod config;
mod control_pipe;
mod device_builder;

/// Prelude for applications.
pub mod prelude {
    pub use crate::device::{IadMode, UsbDevice, UsbDeviceBuilder, UsbDeviceState, UsbVidPid};
    pub use crate::UsbError;
}
