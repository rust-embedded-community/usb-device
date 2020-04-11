use crate::allocator::{InterfaceHandle, StringHandle};
use crate::class::UsbClass;
use crate::endpoint::{EndpointIn, EndpointOut};
use crate::usbcore::UsbCore;
use crate::Result;

// Dynamic dispatch is used to keep the `UsbClass::configure` method object safe and to avoid
// monomorphization.

/// Used by classes to register descriptors and resources with usb-device. See
/// [`UsbClass::configure`](crate::class::UsbClass::configure) for more information on what this
/// type does.
pub struct Config<'v, U: UsbCore>(&'v mut dyn ConfigVisitor<U>);

impl<'v, U: UsbCore> Config<'v, U> {
    pub(crate) fn visit(
        classes: &mut [&mut dyn UsbClass<U>],
        visitor: &mut dyn ConfigVisitor<U>,
    ) -> Result<()> {
        for cls in classes.iter_mut() {
            cls.configure(Config(visitor))?;
        }

        Ok(())
    }

    /// Registers a string descriptor with the specified value.
    #[inline(always)]
    pub fn string(
        &mut self,
        handle: &mut StringHandle,
        string: &str) -> Result<&mut Self>
    {
        self.0.string(handle, string)?;
        Ok(self)
    }

    /// Registers an interface handle and the associated descriptor and begins configuration for the
    /// interface.
    #[inline(always)]
    pub fn interface<'c>(
        &'c mut self,
        interface: &mut InterfaceHandle,
        descriptor: InterfaceDescriptor<'c>,
    ) -> Result<InterfaceConfig<'v, 'c, U>> {
        self.0.begin_interface(interface, &descriptor)?;

        Ok(InterfaceConfig {
            parent: self,
            interface_number: interface.into(),
            descriptor,
        })
    }

    /// Registers an arbitrary (class-specific) descriptor. The descriptor type and length fields
    /// will be written automatically, so `descriptor` should only contain the part following that.
    #[inline(always)]
    pub fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<&mut Self> {
        self.0.descriptor(descriptor_type, descriptor)?;

        Ok(self)
    }
}

/// Used by classes to register an interface's endpoints and alternate settings.
pub struct InterfaceConfig<'v, 'c, U: UsbCore> {
    parent: &'c mut Config<'v, U>,
    interface_number: u8,
    descriptor: InterfaceDescriptor<'c>,
}

impl<U: UsbCore> InterfaceConfig<'_, '_, U> {
    /// Starts the next alternate setting for the interface. If your interface doesn't have any
    /// alternate setting, you shouldn't call this method.
    pub fn next_alt_setting(&mut self) -> Result<&mut Self> {
        self.parent
            .0
            .next_alt_setting(self.interface_number, &self.descriptor)?;
        Ok(self)
    }

    /// Registers an OUT endpoint in the interface.
    #[inline(always)]
    pub fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>) -> Result<&mut Self> {
        self.parent.0.endpoint_out(endpoint, None)?;
        Ok(self)
    }

    /// Registers an OUT endpoint in the interface with a manually provided endpoint descriptor. The
    /// descriptor type (Endpoint) and length fields will be written automatically, so `descriptor`
    /// should only contain the part following that.
    ///
    /// This should rarely be needed as extended endpoint descriptors have been deprecated.
    #[inline(always)]
    pub fn endpoint_out_manual(
        &mut self,
        endpoint: &mut EndpointOut<U>,
        descriptor: &[u8],
    ) -> Result<&mut Self> {
        self.parent.0.endpoint_out(endpoint, Some(descriptor))?;
        Ok(self)
    }

    /// Registers an OUT endpoint in the interface.
    #[inline(always)]
    pub fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>) -> Result<&mut Self> {
        self.parent.0.endpoint_in(endpoint, None)?;
        Ok(self)
    }

    /// Registers an IN endpoint in the interface with a manually provided endpoint descriptor. The
    /// descriptor type (Endpoint) and length fields will be written automatically, so `descriptor`
    /// should only contain the part following that.
    ///
    /// This should rarely be needed as extended endpoint descriptors have been deprecated.
    #[inline(always)]
    pub fn endpoint_in_manual(
        &mut self,
        endpoint: &mut EndpointIn<U>,
        descriptor: &[u8],
    ) -> Result<&mut Self> {
        self.parent.0.endpoint_in(endpoint, Some(descriptor))?;
        Ok(self)
    }

    /// Registers an arbitrary (class-specific) descriptor. The descriptor type and length fields
    /// will be written automatically, so `descriptor` should only contain the part following that.
    #[inline(always)]
    pub fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<&mut Self> {
        self.parent.0.descriptor(descriptor_type, descriptor)?;
        Ok(self)
    }
}

impl<U: UsbCore> Drop for InterfaceConfig<'_, '_, U> {
    fn drop(&mut self) {
        self.parent.0.end_interface();
    }
}

pub(crate) trait ConfigVisitor<U: UsbCore> {
    fn string(
        &mut self,
        string: &mut StringHandle,
        value: &str) -> Result<()>
    {
        let _ = (string, value);
        Ok(())
    }

    fn begin_interface(
        &mut self,
        interface: &mut InterfaceHandle,
        desc: &InterfaceDescriptor,
    ) -> Result<()> {
        let _ = (interface, desc);
        Ok(())
    }

    fn next_alt_setting(
        &mut self,
        interface_number: u8,
        desc: &InterfaceDescriptor,
    ) -> Result<()> {
        let _ = (interface_number, desc);
        Ok(())
    }

    fn end_interface(&mut self) -> () {}

    fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>, manual: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, manual);
        Ok(())
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, manual: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, manual);
        Ok(())
    }

    fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        let _ = (descriptor_type, descriptor);
        Ok(())
    }
}

/// USB interface descriptor.
#[derive(Copy, Clone)]
pub struct InterfaceDescriptor<'n> {
    pub(crate) class: u8,
    pub(crate) sub_class: u8,
    pub(crate) protocol: u8,
    pub(crate) description: Option<&'n StringHandle>,
}

impl<'n> InterfaceDescriptor<'n> {
    /// Creates a new interface descriptor with the specified class code. Non-standard classes
    /// should use the class code `0xff` (vendor-specific).
    pub const fn class(class: u8) -> Self {
        InterfaceDescriptor {
            class,
            sub_class: 0,
            protocol: 0,
            description: None,
        }
    }

    /// Sets the subclass code of the descriptor. The meaning depends on the class code.
    pub const fn subclass(self, sub_class: u8) -> Self {
        InterfaceDescriptor {
            sub_class,
            ..self
        }
    }

    /// Sets the protocol code of the descriptor. The meaning depends on the class and subclass
    /// codes.
    pub const fn protocol(self, protocol: u8) -> Self {
        InterfaceDescriptor {
            protocol,
            ..self
        }
    }

    /// Sets the interface description string. Use `[Config::string]` to register the content of the
    /// string.
    pub const fn description(self, description: &'n StringHandle) -> InterfaceDescriptor<'n> {
        InterfaceDescriptor {
            description: Some(description),
            ..self
        }
    }
}
