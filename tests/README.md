usb-device testing
==================

Testing `usb-device` involves a test suite running on a host computer, connected via USB to a target computer (microcontroller) which provides a test device.
For the host part, see the `test_class_host` folder.
For the device part, external crates are required since `usb-device` only provides an interface but not any hardware drivers.
Here is a list of hardware implementations of the test suite:

* [stm32-usbd-tests](https://github.com/Disasm/stm32-usbd-tests) and [usb-otg-workspace](https://github.com/Disasm/usb-otg-workspace) for STM32 parts.
* [test-usb-device](https://github.com/ianrrees/test-usb-device) for ATSAMD parts.
