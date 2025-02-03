usb-device
==========

USB stack for embedded devices in Rust.

The UsbDevice object represents a composite USB device and is the most important object for
application implementors. The UsbDevice combines a number of UsbClasses (either custom ones, or
pre-existing ones provided by other crates) and a UsbBus device driver to implement the USB device.

The UsbClass trait can be used to implement USB classes such as a HID device or a serial port. An
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

* [musb](https://github.com/decaday/musb) - device-driver implementation for musb (Mentor USB, USB2.0 IP), widely used in various microcontrollers and SoCs from vendors like TI, MediaTek, Puya, and Allwinner. 
  Examples can be found in [py32-hal](https://github.com/py32-rs/py32-hal/tree/main/examples/usbd-f072).

* [rp2040-hal](https://github.com/rp-rs/rp-hal) -
device-driver implementation for the raspberry pi RP2040
microcontroller. Examples can be found in the various boards
crates [here](https://github.com/rp-rs/rp-hal-boards).

* [stm32-usbd](https://github.com/stm32-rs/stm32-usbd) - device-driver implementation for multiple STM32 microcontroller families.
  Examples can be found in each individual HAL crate that implements the USB peripheral.

Class crates
------------

* [usbd-hid](https://github.com/twitchyliquid64/usbd-hid) [![Crates.io](https://img.shields.io/crates/v/usbd-hid.svg)](https://crates.io/crates/usbd-hid) - HID class
* [usbd-human-interface-device](https://github.com/dlkj/usbd-human-interface-device) [![Crates.io](https://img.shields.io/crates/v/usbd-human-interface-device.svg)](https://crates.io/crates/usbd-human-interface-device) - HID class
* [usbd-serial](https://github.com/rust-embedded-community/usbd-serial) [![Crates.io](https://img.shields.io/crates/v/usbd-serial.svg)](https://crates.io/crates/usbd-serial) - CDC-ACM serial port class
* [usbd-storage](https://github.com/apohrebniak/usbd-storage) [![Crates.io](https://img.shields.io/crates/v/usbd-storage.svg)](https://crates.io/crates/usbd-storage) - (Experimental) Mass storage port class
* [usbd-dfu](https://github.com/vitalyvb/usbd-dfu) [![Crates.io](https://img.shields.io/crates/v/usbd-dfu.svg)](https://crates.io/crates/usbd-dfu) - Device Firmware Upgrade class
* [usbd-picotool-reset](https://github.com/ithinuel/usbd-picotool-reset) [![Crates.io](https://img.shields.io/crates/v/usbd-picotool-reset.svg)](https://crates.io/crates/usbd-picotool-reset) - picotool-reset class
* [usbd-midi](https://github.com/rust-embedded-community/usbd-midi) [![Crates.io](https://img.shields.io/crates/v/usbd-midi.svg)](https://crates.io/crates/usbd-midi) - MIDI class
* [usbd-audio](https://github.com/kiffie/usbd-audio) [![Crates.io](https://img.shields.io/crates/v/usbd-audio.svg)](https://crates.io/crates/usbd-audio) - (Experimental) Audio class

Others
------

Other implementations for USB in Rust

* The [Embassy](https://github.com/embassy-rs/embassy) project has an async USB stack, embassy-usb.
