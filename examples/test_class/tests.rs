use crate::device::*;
use libusb::*;

use rand::prelude::*;

pub type TestFn = fn(&DeviceHandles<'_>) -> ();

macro_rules! tests {
    { $(fn $name:ident($dev:ident) $body:expr)* } => {
        pub fn get_tests() -> Vec<(&'static str, TestFn)> {
            let mut tests: Vec<(&'static str, TestFn)> = Vec::new();

            $(
                fn $name($dev: &DeviceHandles<'_>) {
                    $body
                }

                tests.push((stringify!($name), $name));
            )*

            tests
        }
    }
}

tests! {

fn string_descriptors(dev) {
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

fn control_request(dev) {
    let mut rng = rand::thread_rng();

    let value: u16 = rng.gen();
    let index: u16 = rng.gen();
    let data = random_data(rng.gen_range(0, 16));

    let mut expected = [0u8; 8];
    expected[0] = (0x02 as u8) << 5;
    expected[1] = test_class::REQ_STORE_REQUEST;
    expected[2..4].copy_from_slice(&value.to_le_bytes());
    expected[4..6].copy_from_slice(&index.to_le_bytes());
    expected[6..8].copy_from_slice(&(data.len() as u16).to_le_bytes());

    assert_eq!(
        dev.write_control(
            request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
            test_class::REQ_STORE_REQUEST, value, index,
            &data, TIMEOUT).expect("control write"),
        data.len());

    let mut response = [0u8; 8];

    assert_eq!(
        dev.read_control(
            request_type(Direction::In, RequestType::Vendor, Recipient::Device),
            test_class::REQ_READ_BUFFER, 0, 0,
            &mut response, TIMEOUT).expect("control read"),
        response.len());

    assert_eq!(&response, &expected);
}

fn control_data(dev) {
    for len in &[0, 7, 8, 9, 15, 16, 17] {
        let data = random_data(*len);

        assert_eq!(
            dev.write_control(
                request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
                test_class::REQ_WRITE_BUFFER, 0, 0,
                &data, TIMEOUT).expect(&format!("control write len {}", len)),
            data.len());

        let mut response = vec![0u8; *len];

        assert_eq!(
            dev.read_control(
                request_type(Direction::In, RequestType::Vendor, Recipient::Device),
                test_class::REQ_READ_BUFFER, 0, 0,
                &mut response, TIMEOUT).expect(&format!("control read len {}", len)),
            response.len());

        assert_eq!(&response, &data);
    }
}

fn control_error(dev) {
    let res = dev.write_control(
        request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
        test_class::REQ_UNKNOWN, 0, 0,
        &[], TIMEOUT);

    if res.is_ok() {
        panic!("unknown control request succeeded");
    }
}

}

fn random_data(len: usize) -> Vec<u8> {
    let mut data = vec![0u8; len];
    rand::thread_rng().fill(data.as_mut_slice());
    data
}
