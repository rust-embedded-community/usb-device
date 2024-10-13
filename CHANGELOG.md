# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

* `DummyUsbBus` without functionality to allow examples that actually compile (but not run).
* Extended `UsbRev` enum with variants for USB 1.0 and 1.1.

### Changed

* [breaking] The control pipe is now provided in the `UsbDeviceBuilder` API to allow for user-provided control
pipes. This makes it so that control pipes have configurable sizing.
* Don't require UsbBus to be Sync. If a UsbBus is not Sync, it can still be used to make a UsbDevice, but that UsbDevice will not be Sync (ensuring soundness).

## [0.3.2] - 2024-03-06

### Added

* A new `log` feature can be enabled to provide logging and tracing information about the USB
interface.

### Changed

* [breaking] LangIDs no longer implement `TryFromPrimitive`. The minor version has not been bumped
as this was not intended to be used in the public API.
  * If this is problematic, please open an issue in the main `usb-device` repository.
* Changed handling of EP0 state to eliminate unexpected issues with device enumeration

## [0.3.1] - 2023-11-15

### Added

* `BuilderError`, `LangID`, `StringDescriptors` now in `prelude`
* `LangID` now in `class_prelude`

### Changed

* Updated documentation, including example code

## [0.3.0] - 2023-11-13

### Fixed

* Fixed a defect where enumeration may fail due to timing-related issues ([#128](https://github.com/rust-embedded-community/usb-device/issues/128))

### Added

* New enums and allocators for Isochronous endpoints ([#60](https://github.com/rust-embedded-community/usb-device/pull/60)).
* Ability to select USB revision ([#116](https://github.com/rust-embedded-community/usb-device/pull/116)).
* Added support for alternate settings on interfaces ([#114](https://github.com/rust-embedded-community/usb-device/pull/114)).
* Added support for architectures without atomics ([#115](https://github.com/rust-embedded-community/usb-device/pull/115)).
* Added support for multi-language STRING desc ([#122](https://github.com/rust-embedded-community/usb-device/pull/122)).
  * `UsbDeviceBuilder` has a public `.extra_lang_ids()` method to specify LANGIDs besides ENGLISH_US(0x0409)

### Breaking

* Acess numeric form of `EndpointType` variants now require a `.to_bm_attributes()`. ([#60](https://github.com/rust-embedded-community/usb-device/pull/60))
* `DescriptorWriter::iad()` now requires a `Option<StringIndex>` to optionally specify a string for describing the function ([#121](https://github.com/rust-embedded-community/usb-device/pull/121))
* `.manufacturer()`, `.product()` and `.serial_number()` of `UsbDeviceBuilder` are now replaced with the `strings()` function that accepts a `StringDescriptor` list to allow multilanguage support ([#122](https://github.com/rust-embedded-community/usb-device/pull/122))
* Various methods of the `UsbDeviceBuilder` now return `Result<>` types instead of internally panicking.

### Changed

* `EndpointType` enum now has fields for isochronous synchronization and usage ([#60](https://github.com/rust-embedded-community/usb-device/pull/60)).
* `descriptor_type::STRING` of `fn get_descriptor()` will send the LANGIDs supported by device, and respond STRING Request with specified LANGID. ([#122](https://github.com/rust-embedded-community/usb-device/pull/122))
* `UsbError` is now copyable and comparable ([#127](https://github.com/rust-embedded-community/usb-device/pull/127))

## [0.2.9] - 2022-08-02

### Added

* Optional support for defmt ([#76](https://github.com/rust-embedded-community/usb-device/pull/76)).

### Fixed

* Fixed an issue where USB devices were not enumerating on Windows ([#32](https://github.com/rust-embedded-community/usb-device/issues/82))
* Fixed Suspend state transition so it goes back to the previous state, not just Default ([#97](https://github.com/rust-embedded-community/usb-device/pull/97))

## [0.2.8] - 2021-03-13

## [0.2.7] - 2020-10-03

## [0.2.6] - 2020-09-22

## [0.2.5] - 2020-02-10

## [0.2.4] - 2020-02-01

## [0.2.3] - 2019-08-28

## [0.2.2] - 2019-07-27

## [0.2.1] - 2019-06-07

## [0.2.0] - 2019-06-07

## 0.1.0 - 2019-06-07

This is the initial release to crates.io.

[Unreleased]: https://github.com/rust-embedded-community/usb-device/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/rust-embedded-community/usb-device/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/rust-embedded-community/usb-device/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.9...v0.3.0
[0.2.9]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rust-embedded-community/usb-device/compare/v0.1.0...v0.2.0
