use crate::allocator::{InterfaceHandle, StringHandle};
use crate::endpoint::{EndpointIn, EndpointOut};
use crate::usbcore::UsbCore;
use crate::Result;

/// Used by classes to register descriptors and resources with usb-device. See
/// [`UsbClass::configure`](crate::class::UsbClass::configure) for more information on what this
/// type does.
pub struct Config<'v, U: UsbCore>(pub(crate) &'v mut dyn ConfigVisitor<U>);

impl<'v, U: UsbCore> Config<'v, U> {
    pub(crate) fn internal_clone(&mut self) -> Config<U> {
        Config(self.0)
    }

    /// Registers a string descriptor with the specified value.
    #[inline(always)]
    pub fn string(&mut self, handle: &mut StringHandle, string: &str) -> Result<&mut Self> {
        self.0.string(handle, string)?;
        Ok(self)
    }

    /// Creates an interface association descriptor for functions that use multiple linked
    /// interfaces. You should store the result of this in a variable in a new scope to make the
    /// lifetimes work and use it to define your interfaces.
    #[inline(always)]
    pub fn interface_association(
        &mut self,
        descriptor: InterfaceDescriptor,
    ) -> Result<InterfaceAssociationConfig<'v, '_, U>> {
        self.0.begin_interface(None, &descriptor)?;

        Ok(InterfaceAssociationConfig { parent: self })
    }

    /// Registers an interface handle and the associated descriptor and begins configuration for the
    /// interface.
    #[inline(always)]
    pub fn interface(
        &mut self,
        interface: &mut InterfaceHandle,
        descriptor: InterfaceDescriptor,
    ) -> Result<InterfaceConfig<'v, '_, U>> {
        self.0.begin_interface(Some(interface), &descriptor)?;

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
pub struct InterfaceAssociationConfig<'v, 'c, U: UsbCore> {
    parent: &'c mut Config<'v, U>,
}

impl<'v, 'c, U: UsbCore> InterfaceAssociationConfig<'v, 'c, U> {
    /// Registers an interface handle and the associated descriptor and begins configuration for the
    /// interface.
    #[inline(always)]
    pub fn interface(
        &mut self,
        interface: &mut InterfaceHandle,
        descriptor: InterfaceDescriptor,
    ) -> Result<InterfaceConfig<'v, '_, U>> {
        self.parent
            .0
            .begin_interface(Some(interface), &descriptor)?;

        Ok(InterfaceConfig {
            parent: self.parent,
            interface_number: interface.into(),
            descriptor,
        })
    }

    /// Registers an arbitrary (class-specific) descriptor. The descriptor type and length fields
    /// will be written automatically, so `descriptor` should only contain the part following that.
    #[inline(always)]
    pub fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<&mut Self> {
        self.parent.0.descriptor(descriptor_type, descriptor)?;

        Ok(self)
    }
}

impl<U: UsbCore> Drop for InterfaceAssociationConfig<'_, '_, U> {
    fn drop(&mut self) {
        self.parent.0.end_interface(true);
    }
}

/// Used by classes to register an interface's endpoints and alternate settings.
pub struct InterfaceConfig<'v, 'c, U: UsbCore> {
    parent: &'c mut Config<'v, U>,
    interface_number: u8,
    descriptor: InterfaceDescriptor,
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
        self.parent.0.end_interface(false);
    }
}

pub(crate) trait ConfigVisitor<U: UsbCore> {
    /// Called for each string descriptor
    #[inline(always)]
    fn string(&mut self, string: &mut StringHandle, value: &str) -> Result<()> {
        let _ = (string, value);
        Ok(())
    }

    /// Called at the start of each interface or IAD. If `interface` is `Some`, this is the start of
    /// an interface, otherwise this is an IAD.
    #[inline(always)]
    fn begin_interface(
        &mut self,
        interface: Option<&mut InterfaceHandle>,
        desc: &InterfaceDescriptor,
    ) -> Result<()> {
        let _ = (interface, desc);
        Ok(())
    }

    /// Called between alt settings in each interface (not for the first alt setting)
    #[inline(always)]
    fn next_alt_setting(&mut self, interface_number: u8, desc: &InterfaceDescriptor) -> Result<()> {
        let _ = (interface_number, desc);
        Ok(())
    }

    /// Called at the end of each interface.
    #[inline(always)]
    fn end_interface(&mut self, iad: bool) -> () {
        let _ = iad;
    }

    /// Called for each OUT endpoint. `manual` is an optional manually supplied descriptor.
    #[inline(always)]
    fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>, manual: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, manual);
        Ok(())
    }

    /// Called for each IN endpoint. `manual` is an optional manually supplied descriptor.
    #[inline(always)]
    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, manual: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, manual);
        Ok(())
    }

    /// Called for each arbitrary descriptor.
    #[inline(always)]
    fn descriptor(&mut self, descriptor_type: u8, descriptor: &[u8]) -> Result<()> {
        let _ = (descriptor_type, descriptor);
        Ok(())
    }
}

/// USB interface descriptor.
#[derive(Copy, Clone)]
pub struct InterfaceDescriptor {
    pub(crate) class: u8,
    pub(crate) sub_class: u8,
    pub(crate) protocol: u8,
    pub(crate) description: u8,
}

impl<'n> InterfaceDescriptor {
    /// Creates a new interface descriptor with the specified class code. Non-standard classes
    /// should use the class code `0xff` (vendor-specific).
    pub const fn class(class: u8) -> Self {
        InterfaceDescriptor {
            class,
            sub_class: 0,
            protocol: 0,
            description: 0,
        }
    }

    /// Sets the sub-class code of the descriptor. The meaning depends on the class code.
    pub const fn sub_class(self, sub_class: u8) -> Self {
        InterfaceDescriptor { sub_class, ..self }
    }

    /// Sets the protocol code of the descriptor. The meaning depends on the class and sub-class
    /// codes.
    pub const fn protocol(self, protocol: u8) -> Self {
        InterfaceDescriptor { protocol, ..self }
    }

    /// Sets the interface description string. Use `[Config::string]` to register the content of the
    /// string.
    pub fn description(self, description: &StringHandle) -> InterfaceDescriptor {
        InterfaceDescriptor {
            description: description.index(),
            ..self
        }
    }
}
