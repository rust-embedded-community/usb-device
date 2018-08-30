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
//! implementation crates. The main object for end-users is [`device::UsbDevice`]. That, and related
//! types are available with `use usb_device::prelude::*`.
//!
//! ## Class implementors
//!
//! Class implementors can implement support new USB classes by using [`class::UsbClass`]. All
//! class-related types can be easily imported with `use usb_device::class_prelude::*`.
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
//! transfer refers to host-to-device transfer, and an IN transfer refers to device-to-host
//! transfer. This is mainly a concern for implementing new USB peripheral drivers and USB classes,
//! and people doing that should be familiar with the USB standard.

#![no_std]

/// A USB stack error.
#[derive(Debug)]
pub enum UsbError {
    EndpointOverflow,
    SizeOverflow,
    InvalidEndpoint,
    InvalidSetupPacket,
    EndpointTaken,
    NoData,
    Busy,
    BufferOverflow,
}

/// Result for USB operations.
pub type Result<T> = core::result::Result<T, UsbError>;

/// USB control transfers and the SETUP packet.
pub mod control;
/// For implementing peripheral drivers.
pub mod bus;
/// For implementing USB classes.
pub mod class;
/// USB endpoints.
pub mod endpoint;
mod device;
mod descriptor;
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