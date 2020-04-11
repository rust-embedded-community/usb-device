# usb-device

[![crates.io](https://meritbadge.herokuapp.com/usb-device)](https://crates.io/crates/usb-device) [![documentation](https://docs.rs/usb-device/badge.svg)](https://docs.rs/usb-device)

Experimental device-side USB stack for embedded devices in Rust.

This crate is still under development and should not be considered production ready or even USB
compliant.

## [Documentation](http://docs.rs/usb-device/)

See the documentation for more details, as well for an example of implementing a USB class!

## Example

This is an example of a USB application.

```rust
    use usb_device::prelude::*; // the prelude is geared towards creating USB applications
    use usb_device::test_class::TestClass; // using the built-in test class as a sample

    // Create the USB peripheral driver. The exact name and arguments are platform specific, so
    // check the documentation for your device driver crate.
    let usb = device_specific_usb::UsbCore::new();

    // Create one or more USB classes.
    let mut usb_class = TestClass::new();

    // Build the final UsbDevice instance. The required arguments are the peripheral driver
    // created earlier, as well as a USB vendor ID/product ID pair. Additional builder arguments
    // can specify parameters such as device class code or product name. If using an existing
    // class,remember to check the class crate documentation for correct values for fields such
    // as device_class.
    let mut usb_dev = UsbDeviceBuilder::new(usb, UsbVidPid(0x5824, 0x27dd))
        .device_class(0xff) // vendor specific
        .product("My product")
        .build(&mut [&mut usb_class])
        .expect("device creation failed");

    // At this point the USB peripheral is enabled and a the host will attempt to enumerate it if
    // it's plugged in.
    loop {
        // Must be called more often than once every 10ms to handle events and stay USB compliant,
        // or from a device-specific interrupt handler. The list of classes must be the same
        // classes in the same order as at device creation time.
        if usb_dev.poll(&mut [&mut usb_class]).is_ok() {
            // Call class-specific methods here
        }
    }
```

# Hardware driver crates

* [stm32-usbd](https://github.com/stm32-rs/stm32-usbd) - device-driver implementation for multiple STM32 microcontroller families.
  Examples can be found in [stm32-usbd-examples](https://github.com/stm32-rs/stm32-usbd-examples).

* [atsamd](https://github.com/atsamd-rs/atsamd) - device-driver implementation for samd21 & samd51 microcontrollers. An example for the
  itsybitsy_m4 board from Adafruit can be found [here](https://github.com/atsamd-rs/atsamd/blob/master/boards/itsybitsy_m4/examples/usb_serial.rs).

## Class crates

* [usbd-serial](https://github.com/mvirkkunen/usbd-serial) [![Crates.io](https://img.shields.io/crates/v/usbd-serial.svg)](https://crates.io/crates/usbd-serial) - CDC-ACM serial port class
* [usbd-hid](https://github.com/twitchyliquid64/usbd-hid) [![Crates.io](https://img.shields.io/crates/v/usbd-hid.svg)](https://crates.io/crates/usbd-hid) - HID class

## TODO

Features planned but not implemented yet:

- Isochronous endpoints
- Multilingual string descriptors

Features not planning to support at the moment:

- More than one configuration descriptor (uncommon in practice)
