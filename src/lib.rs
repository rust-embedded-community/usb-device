#![no_std]

#[derive(Debug)]
pub enum UsbError {
    EndpointOverflow,
    SizeOverflow,
    InvalidEndpoint,
    InvalidSetupPacket,
    InvalidHandle,
    NoData,
    Busy,
    BufferOverflow,
}

pub type Result<T> = core::result::Result<T, UsbError>;

pub mod control;
pub mod bus;
pub mod class;
mod device;
mod descriptor;
mod device_info;
mod device_standard_control;

pub use bus::{UsbBus, EndpointType, EndpointPair, EndpointIn, EndpointOut};
pub use device::{UsbDevice, DeviceState};
pub use device_info::UsbDeviceInfo;
pub use descriptor::DescriptorWriter;
