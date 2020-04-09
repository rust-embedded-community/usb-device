use crate::config::{ConfigVisitor, InterfaceDescriptor};
use crate::endpoint::{EndpointCore, EndpointIn, EndpointOut};
use crate::usbcore::{UsbCore, UsbEndpointAllocator};
use crate::{Result, UsbError};

// Reserved numbers for standard descriptor strings
pub(crate) const MANUFACTURER_STRING: u8 = 1;
pub(crate) const PRODUCT_STRING: u8 = 2;
pub(crate) const SERIAL_NUMBER_STRING: u8 = 3;
const FIRST_ALLOCATED_STRING: u8 = 4;

/// Allocates resources for USB classes.
pub(crate) struct UsbAllocator<'a, U: UsbCore> {
    ep_alloc: &'a mut U::EndpointAllocator,
    next_string: u8,
    next_interface: u8,
}

impl<U: UsbCore> UsbAllocator<'_, U> {
    pub(crate) fn new(ep_alloc: &mut U::EndpointAllocator) -> UsbAllocator<U> {
        UsbAllocator {
            ep_alloc,
            next_string: FIRST_ALLOCATED_STRING,
            next_interface: 0,
        }
    }
}

impl<U: UsbCore> ConfigVisitor<U> for UsbAllocator<'_, U> {
    fn string(&mut self, string: &mut StringHandle) -> Result<()> {
        if cfg!(debug_assertions) && string.0.is_some() {
            return Err(UsbError::DuplicateConfig);
        }

        string.0 = Some(self.next_string);
        self.next_string += 1;

        Ok(())
    }

    fn begin_interface(
        &mut self,
        interface: &mut InterfaceHandle,
        _descriptor: &InterfaceDescriptor,
    ) -> Result<()> {
        if cfg!(debug_assertions) && interface.0.is_some() {
            return Err(UsbError::DuplicateConfig);
        }

        interface.0 = Some(self.next_interface);
        self.next_interface += 1;

        Ok(())
    }

    fn endpoint_out(
        &mut self,
        endpoint: &mut EndpointOut<U>,
        _manual: Option<&[u8]>,
    ) -> Result<()> {
        if cfg!(debug_assertions) && endpoint.core.is_some() {
            return Err(UsbError::DuplicateConfig);
        }

        endpoint.core = Some(EndpointCore {
            enabled: false,
            ep: self.ep_alloc.alloc_out(&endpoint.config)?,
        });

        Ok(())
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, _manual: Option<&[u8]>) -> Result<()> {
        if cfg!(debug_assertions) && endpoint.core.is_some() {
            return Err(UsbError::DuplicateConfig);
        }

        endpoint.core = Some(EndpointCore {
            enabled: false,
            ep: self.ep_alloc.alloc_in(&endpoint.config)?,
        });

        Ok(())
    }
}

/// A handle for a USB interface that contains its number.
#[derive(Default)]
pub struct InterfaceHandle(pub(crate) Option<u8>);

impl InterfaceHandle {
    pub const fn new() -> Self {
        InterfaceHandle(None)
    }
}

impl From<&InterfaceHandle> for u8 {
    fn from(handle: &InterfaceHandle) -> u8 {
        handle.0.unwrap_or(0)
    }
}

impl From<&mut InterfaceHandle> for u8 {
    fn from(handle: &mut InterfaceHandle) -> u8 {
        handle.0.unwrap_or(0)
    }
}

impl PartialEq for InterfaceHandle {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for InterfaceHandle {}

/// A handle for a USB string descriptor that contains its index.
#[derive(Default)]
pub struct StringHandle(pub(crate) Option<u8>);

impl StringHandle {
    pub const fn new() -> Self {
        StringHandle(None)
    }
}

impl From<&StringHandle> for u8 {
    fn from(handle: &StringHandle) -> u8 {
        handle.0.unwrap_or(0)
    }
}

impl From<&mut StringHandle> for u8 {
    fn from(handle: &mut StringHandle) -> u8 {
        handle.0.unwrap_or(0)
    }
}

impl PartialEq for StringHandle {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for StringHandle {}
