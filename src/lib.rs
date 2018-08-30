#![no_std]

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

pub type Result<T> = core::result::Result<T, UsbError>;

pub mod control;
pub mod bus;
pub mod class;
pub mod endpoint;
mod device;
mod descriptor;
mod device_builder;
mod device_standard_control;

pub mod prelude {
    pub use ::UsbError;
    pub use ::device::{UsbDevice, UsbDeviceState, UsbDeviceBuilder, UsbVidPid};
}

pub mod class_prelude {
    pub use ::UsbError;
    pub use ::bus::{UsbBus, UsbAllocator, InterfaceNumber, StringIndex};
    pub use ::device::{ControlOutResult, ControlInResult};
    pub use ::descriptor::DescriptorWriter;
    pub use ::endpoint::{EndpointType, EndpointIn, EndpointOut};
    pub use ::class::UsbClass;
    pub use ::control;
}