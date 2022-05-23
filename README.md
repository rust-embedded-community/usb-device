usb-device
==========

Experimental device-side USB stack for embedded devices in Rust.

This crate is still under development and should not be considered production ready or even USB
compliant.

The UsbDevice object represents a composite USB device and is the most important object for
application implementors. The UsbDevice combines a number of UsbClasses (either custom ones, or
pre-existing ones provided by other crates) and a UsbBus device drives to implement the USB device.

The UsbClass trait can be used to implemented USB classes such as a HID device or a serial port. An
implementation may also use a custom class if the required functionality isn't covered by a standard
class.

The UsbBus trait is intended to be implemented by device-specific crates to provide a driver for
each device's USB peripheral.

Hardware driver crates
----------------------

* [atsam4](https://github.com/atsam-rs/atsam4-hal) - device-driver implementation for atsam4e & atsam4s microcontrollers (UDP). Examples can be found [here](https://github.com/atsam-rs/sam_xplained). While not expressly supported with this crate, atsam3s and atsam55g could also be supported with a similar code-base.

* [atsamd](https://github.com/atsamd-rs/atsamd) - device-driver implementation for samd21 & samd51 microcontrollers. An example for the
  itsybitsy_m4 board from Adafruit can be found [here](https://github.com/atsamd-rs/atsamd/blob/master/boards/itsybitsy_m4/examples/usb_serial.rs).

* [imxrt-usbd](https://github.com/imxrt-rs/imxrt-usbd) - device-driver implementation for NXP i.MX RT microcontrollers. Examples for
  i.MX RT boards, like the Teensy 4, are maintained with the driver.

* [stm32-usbd](https://github.com/stm32-rs/stm32-usbd) - device-driver implementation for multiple STM32 microcontroller families.
  Examples can be found in each individual HAL crate that implements the USB peripheral.

Class crates
------------

* [usbd-hid](https://github.com/twitchyliquid64/usbd-hid) [![Crates.io](https://img.shields.io/crates/v/usbd-hid.svg)](https://crates.io/crates/usbd-hid) - HID class
* [usbd-serial](https://github.com/mvirkkunen/usbd-serial) [![Crates.io](https://img.shields.io/crates/v/usbd-serial.svg)](https://crates.io/crates/usbd-serial) - CDC-ACM serial port class


Others
------

Other implementations for USB in Rust
* [embassy-usb](https://github.com/embassy-rs/embassy/blob/master/embassy-usb/src/driver.rs), an async variant.