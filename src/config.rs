use crate::allocator::{InterfaceHandle, StringHandle};
use crate::class::UsbClass;
use crate::endpoint::{EndpointIn, EndpointOut};
use crate::usbcore::UsbCore;
use crate::Result;

// Dynamic dispatch is used to keep the `UsbClass::configure` method object safe and to avoid
// monomorphization.
pub struct Config<'v, U: UsbCore>(&'v mut dyn ConfigVisitor<U>);

impl<'v, U: UsbCore> Config<'v, U> {
    pub(crate) fn visit(
        classes: &mut [&mut dyn UsbClass<U>],
        visitor: &mut impl ConfigVisitor<U>,
    ) -> Result<()> {
        for cls in classes.iter_mut() {
            cls.configure(Config(visitor))?;
        }

        Ok(())
    }

    #[inline(always)]
    pub fn string(&mut self, string: &mut StringHandle) -> Result<&mut Self> {
        self.0.string(string)?;
        Ok(self)
    }

    #[inline(always)]
    pub fn interface<'c>(
        &'c mut self,
        interface: &mut InterfaceHandle,
        descriptor: InterfaceDescriptor,
    ) -> Result<InterfaceConfig<'v, 'c, U>> {
        self.0.begin_interface(interface, &descriptor)?;

        Ok(InterfaceConfig {
            parent: self,
            interface: InterfaceHandle(interface.0),
            descriptor,
        })
    }

    #[inline(always)]
    pub fn descriptor(&mut self, descriptor: &[u8]) -> Result<&mut Self> {
        self.0.descriptor(descriptor)?;

        Ok(self)
    }
}

pub struct InterfaceConfig<'v, 'c, U: UsbCore> {
    parent: &'c mut Config<'v, U>,
    interface: InterfaceHandle,
    descriptor: InterfaceDescriptor,
}

impl<U: UsbCore> InterfaceConfig<'_, '_, U> {
    pub fn alt_setting(&mut self) -> Result<&mut Self> {
        self.parent
            .0
            .next_alt_setting(&mut self.interface, &self.descriptor)?;
        Ok(self)
    }

    #[inline(always)]
    pub fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>) -> Result<&mut Self> {
        self.parent.0.endpoint_out(endpoint, None)?;
        Ok(self)
    }

    #[inline(always)]
    pub fn endpoint_out_ex(
        &mut self,
        endpoint: &mut EndpointOut<U>,
        extra: &[u8],
    ) -> Result<&mut Self> {
        self.parent.0.endpoint_out(endpoint, Some(extra))?;
        Ok(self)
    }

    #[inline(always)]
    pub fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>) -> Result<&mut Self> {
        self.parent.0.endpoint_in(endpoint, None)?;
        Ok(self)
    }

    #[inline(always)]
    pub fn endpoint_in_ex(
        &mut self,
        endpoint: &mut EndpointIn<U>,
        extra: &[u8],
    ) -> Result<&mut Self> {
        self.parent.0.endpoint_in(endpoint, Some(extra))?;
        Ok(self)
    }

    #[inline(always)]
    pub fn descriptor(&mut self, descriptor: &[u8]) -> Result<&mut Self> {
        self.parent.0.descriptor(descriptor)?;
        Ok(self)
    }
}

impl<U: UsbCore> Drop for InterfaceConfig<'_, '_, U> {
    fn drop(&mut self) {
        self.parent.0.end_interface();
    }
}

pub(crate) trait ConfigVisitor<U: UsbCore> {
    fn string(&mut self, string: &mut StringHandle) -> Result<()> {
        let _ = string;
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
        interface: &mut InterfaceHandle,
        desc: &InterfaceDescriptor,
    ) -> Result<()> {
        let _ = (interface, desc);
        Ok(())
    }

    fn end_interface(&mut self) -> () {}

    fn endpoint_out(&mut self, endpoint: &mut EndpointOut<U>, extra: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, extra);
        Ok(())
    }

    fn endpoint_in(&mut self, endpoint: &mut EndpointIn<U>, extra: Option<&[u8]>) -> Result<()> {
        let _ = (endpoint, extra);
        Ok(())
    }

    fn descriptor(&mut self, descriptor: &[u8]) -> Result<()> {
        let _ = descriptor;
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct InterfaceDescriptor {
    pub class: u8,
    pub sub_class: u8,
    pub protocol: u8,
}
