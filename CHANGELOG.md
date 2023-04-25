# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
* New enums and allocators for Isochronous endpoints ([#60](https://github.com/rust-embedded-community/usb-device/pull/60)).
* Ability to select USB revision ([#116](https://github.com/rust-embedded-community/usb-device/pull/116)).
* Added support for alternate settings on interfaces ([#114](https://github.com/rust-embedded-community/usb-device/pull/114)).
* Added support for architectures without atomics ([#115](https://github.com/rust-embedded-community/usb-device/pull/115)).

### Changed
* `EndpointType` enum now has fields for isochronous synchronization and usage ([#60](https://github.com/rust-embedded-community/usb-device/pull/60)).

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

[Unreleased]: https://github.com/rust-embedded-community/usb-device/compare/v0.2.9...HEAD
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
