extern crate usb_device;

mod test_helpers;
use crate::test_helpers::*;

use libusb::*;

// This abuses the integration test system to run tests against actual hardware. One side effect is
// that parallel test running is not possible since the tests talk to actual hardware. To prevent
// people from even trying, the USB device is behind a mutex.

#[test]
fn open_device() {
    let _ = get_device();
}

#[test]
fn string_descriptors() {
    let dev = get_device();

    assert_eq!(
        dev.read_product_string(dev.en_us, &dev.descriptor, TIMEOUT)
            .expect("read product string"),
        test_class::PRODUCT);

    assert_eq!(
        dev.read_manufacturer_string(dev.en_us, &dev.descriptor, TIMEOUT)
            .expect("read manufacturer string"),
        test_class::MANUFACTURER);

    assert_eq!(
        dev.read_serial_number_string(dev.en_us, &dev.descriptor, TIMEOUT)
            .expect("read serial number string"),
        test_class::SERIAL_NUMBER);

    assert_eq!(
        dev.read_string_descriptor(dev.en_us, 4, TIMEOUT)
            .expect("read custom string"),
        test_class::CUSTOM_STRING);
}

#[test]
fn control_no_data() {
    const VALUE: u16 = 0x1337;
    let dev = get_device();

    dev.write_control(
        request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
        test_class::REQ_SET_VALUE, VALUE, 0,
        &[], TIMEOUT).expect("control write");

    let mut response = [0u8; 2];

    dev.read_control(
        request_type(Direction::In, RequestType::Vendor, Recipient::Device),
        test_class::REQ_GET_VALUE, VALUE, 0,
        &mut response, TIMEOUT).expect("control read");

    assert_eq!(response, VALUE.to_le_bytes());
}

#[test]
fn control_error() {
    let dev = get_device();

    let res = dev.write_control(
        request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
        test_class::REQ_UNKNOWN, 0, 0,
        &[], TIMEOUT);

    if res.is_ok() {
        panic!("unknown control request succeeded");
    }
}