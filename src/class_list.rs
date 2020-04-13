use crate::allocator::InterfaceHandle;
use crate::class::{ControlIn, ControlOut, PollEvent, UsbClass};
use crate::config::Config;
use crate::descriptor::BosWriter;
use crate::usbcore::UsbCore;
use crate::Result;

impl<U, C> UsbClass<U> for &mut C
where
    U: UsbCore,
    C: UsbClass<U>,
{
    fn configure(&mut self, config: Config<U>) -> Result<()> {
        (*self).configure(config)
    }

    fn get_bos_descriptors(&mut self, writer: &mut BosWriter) -> Result<()> {
        (*self).get_bos_descriptors(writer)
    }

    fn reset(&mut self) {
        (*self).reset()
    }

    fn alt_setting_activated(&mut self, interface: InterfaceHandle, alt_setting: u8) {
        (*self).alt_setting_activated(interface.internal_clone(), alt_setting);
    }

    fn poll(&mut self, event: &PollEvent) {
        (*self).poll(event);
    }

    fn control_out(&mut self, mut xfer: ControlOut<U>) {
        (*self).control_out(xfer.internal_clone());
    }

    fn control_in(&mut self, mut xfer: ControlIn<U>) {
        (*self).control_in(xfer.internal_clone());
    }
}

macro_rules! tuple_impls {
    ($($n:tt: $c:ident),+) => {
        impl<U, $($c),+> UsbClass<U> for ($(&mut $c),+,)
        where
            U: UsbCore,
            $($c: UsbClass<U>),+,
        {
            fn configure(&mut self, mut config: Config<U>) -> Result<()> {
                $(
                    UsbClass::configure(self.$n, config.internal_clone())?;
                )*

                Ok(())
            }

            fn get_bos_descriptors(&mut self, writer: &mut BosWriter) -> Result<()> {
                $(
                    UsbClass::get_bos_descriptors(self.$n, writer)?;
                )*

                Ok(())
            }

            fn reset(&mut self) {
                $(
                    UsbClass::reset(self.$n);
                )*
            }

            fn alt_setting_activated(&mut self, interface: InterfaceHandle, alt_setting: u8) {
                $(
                    UsbClass::alt_setting_activated(self.$n, interface.internal_clone(), alt_setting);
                )*
            }

            fn poll(&mut self, event: &PollEvent) {
                $(
                    UsbClass::poll(self.$n, event);
                )*
            }

            fn control_out(&mut self, mut xfer: ControlOut<U>) {
                $(
                    UsbClass::control_out(self.$n, xfer.internal_clone());
                )*
            }

            fn control_in(&mut self, mut xfer: ControlIn<U>) {
                $(
                    UsbClass::control_in(self.$n, xfer.internal_clone());
                )*
            }
        }
    }
}

tuple_impls!(0: C0);
tuple_impls!(0: C0, 1: C1);
tuple_impls!(0: C0, 1: C1, 2: C2);
tuple_impls!(0: C0, 1: C1, 2: C2, 3: C3);
tuple_impls!(0: C0, 1: C1, 2: C2, 3: C3, 4: C4);
tuple_impls!(0: C0, 1: C1, 2: C2, 3: C3, 4: C4, 5: C5);
tuple_impls!(0: C0, 1: C1, 2: C2, 3: C3, 4: C4, 5: C5, 6: C6);
tuple_impls!(0: C0, 1: C1, 2: C2, 3: C3, 4: C4, 5: C5, 6: C6, 7: C7);

/// Wrapper type for `UsbClass` trait objects if you want to use dynamic dispatch. Usually this is
/// not needed.
pub struct DynamicClasses<'c, U>(pub &'c mut [&'c mut dyn UsbClass<U>]);

impl<U> UsbClass<U> for DynamicClasses<'_, U>
where
    U: UsbCore,
{
    fn configure(&mut self, mut config: Config<U>) -> Result<()> {
        for cls in self.0.iter_mut() {
            cls.configure(config.internal_clone())?;
        }

        Ok(())
    }

    fn get_bos_descriptors(&mut self, writer: &mut BosWriter) -> Result<()> {
        for cls in self.0.iter_mut() {
            cls.get_bos_descriptors(writer)?;
        }

        Ok(())
    }

    fn reset(&mut self) {
        for cls in self.0.iter_mut() {
            cls.reset();
        }
    }

    fn alt_setting_activated(&mut self, interface: InterfaceHandle, alt_setting: u8) {
        for cls in self.0.iter_mut() {
            cls.alt_setting_activated(interface.internal_clone(), alt_setting);
        }
    }

    fn poll(&mut self, event: &PollEvent) {
        for cls in self.0.iter_mut() {
            cls.poll(event);
        }
    }

    fn control_out(&mut self, mut xfer: ControlOut<U>) {
        for cls in self.0.iter_mut() {
            cls.control_out(xfer.internal_clone());
        }
    }

    fn control_in(&mut self, mut xfer: ControlIn<U>) {
        for cls in self.0.iter_mut() {
            cls.control_in(xfer.internal_clone());
        }
    }
}
