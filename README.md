usb-device
==========

Experimental device-side USB stack for embedded devices in Rust.

This crate is still under development and should not be considered production ready or even USB
compliant.

The UsbDevice object represents a composite USB device and is the most important object for
end-users. Most of the other items are for implementing new USB classes or device-specific drivers.

The UsbClass trait can be used to implemented USB classes such as HID devices or serial ports.
Pre-made class implementations will be provided in separate crates.

The UsbBus trait is intended to be implemented by device-specific crates to provide a driver for
each device specific USB peripheral.

Related crates
--------------

* [stm32f103xx-usb](https://github.com/mvirkkunen/stm32f103xx-usb) - device-driver implementation
  for STM32F103 microcontrollers. Also contains runnable examples.

TODO
----

Features planned but not implemented yet:

- Interface alternate settings
- A safer DescriptorWriter
- Multilingual string descriptors
- Isochronous endpoints

Features not planning to support at the moment:

- More than one configuration descriptor (uncommon in practice)