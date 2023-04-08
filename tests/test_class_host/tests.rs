use crate::device::*;
use rand::prelude::*;
use rusb::{request_type, Direction, Recipient, RequestType, TransferType};
use std::cmp::max;
use std::fmt::Write;
use std::time::{Duration, Instant};
use usb_device::test_class;

pub type TestFn = fn(&mut DeviceHandles, &mut String) -> ();

const BENCH_TIMEOUT: Duration = Duration::from_secs(10);

macro_rules! tests {
    { $(fn $name:ident($dev:ident, $out:ident) $body:expr)* } => {
        pub fn get_tests() -> Vec<(&'static str, TestFn)> {
            let mut tests: Vec<(&'static str, TestFn)> = Vec::new();

            $(
                fn $name($dev: &mut DeviceHandles, $out: &mut String) {
                    $body
                }

                tests.push((stringify!($name), $name));
            )*

            tests
        }
    }
}

tests! {

fn control_request(dev, _out) {
    let mut rng = rand::thread_rng();

    let value: u16 = rng.gen();
    let index: u16 = rng.gen();
    let data = random_data(rng.gen_range(0..16));

    let mut expected = [0u8; 8];
    expected[0] = 0x02_u8 << 5;
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

fn control_data(dev, _out) {
    for len in &[0, 7, 8, 9, 15, 16, 17] {
        let data = random_data(*len);

        assert_eq!(
            dev.write_control(
                request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
                test_class::REQ_WRITE_BUFFER, 0, 0,
                &data, TIMEOUT).unwrap_or_else(|_| panic!("control write len {}", len)),
            data.len());

        let mut response = vec![0u8; *len];

        assert_eq!(
            dev.read_control(
                request_type(Direction::In, RequestType::Vendor, Recipient::Device),
                test_class::REQ_READ_BUFFER, 0, 0,
                &mut response, TIMEOUT).unwrap_or_else(|_| panic!("control read len {}", len)),
            data.len());

        assert_eq!(&response, &data);
    }
}

fn control_data_static(dev, _out) {
    let mut response = [0u8; 257];

    assert_eq!(
        dev.read_control(
            request_type(Direction::In, RequestType::Vendor, Recipient::Device),
            test_class::REQ_READ_LONG_DATA, 0, 0,
            &mut response, TIMEOUT).expect("control read"),
        response.len());

    assert_eq!(&response[..], test_class::LONG_DATA);
}

fn control_error(dev, _out) {
    let res = dev.write_control(
        request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
        test_class::REQ_UNKNOWN, 0, 0,
        &[], TIMEOUT);

    if res.is_ok() {
        panic!("unknown control request succeeded");
    }
}

fn string_descriptors(dev, _out) {
    assert_eq!(
        dev.read_product_string(dev.en_us, &dev.device_descriptor, TIMEOUT)
            .expect("read product string"),
        test_class::PRODUCT);

    assert_eq!(
        dev.read_manufacturer_string(dev.en_us, &dev.device_descriptor, TIMEOUT)
            .expect("read manufacturer string"),
        test_class::MANUFACTURER);

    assert_eq!(
        dev.read_serial_number_string(dev.en_us, &dev.device_descriptor, TIMEOUT)
            .expect("read serial number string"),
        test_class::SERIAL_NUMBER);

    assert_eq!(
        dev.read_string_descriptor(dev.en_us, 4, TIMEOUT)
            .expect("read custom string"),
        test_class::CUSTOM_STRING);
}

fn interface_descriptor(dev, _out) {
    let iface = dev.config_descriptor
        .interfaces()
        .find(|i| i.number() == 0)
        .expect("interface not found");

    let default_alt_setting = iface.descriptors()
        .find(|i| i.setting_number() == 0)
        .expect("default alt setting not found");

    assert_eq!(default_alt_setting.description_string_index(), None);
    assert_eq!(default_alt_setting.class_code(), 0xff);
    assert_eq!(default_alt_setting.sub_class_code(), 0x00);

    let second_alt_setting = iface.descriptors()
        .find(|i| i.setting_number() == 1)
        .expect("second alt setting not found");

    assert_eq!(second_alt_setting.class_code(), 0xff);
    assert_eq!(second_alt_setting.sub_class_code(), 0x01);

    let string_index = second_alt_setting.description_string_index()
        .expect("second alt setting string is undefined");

    assert_eq!(
        dev.read_string_descriptor(dev.en_us, string_index, TIMEOUT)
            .expect("read interface string"),
        test_class::INTERFACE_STRING);
}

fn iso_endpoint_descriptors(dev, _out) {
    // Tests that an isochronous endpoint descriptor is present in the first
    // alternate setting, but not in the default setting.
    let iface = dev.config_descriptor
        .interfaces()
        .find(|i| i.number() == 0)
        .expect("interface not found");

    let mut iso_ep_count = 0;
    for iface_descriptor in iface.descriptors() {
        if iface_descriptor.setting_number() == 0 {
            // Default setting - no isochronous endpoints allowed.  Per USB 2.0
            // spec rev 2.0, 5.6.3 Isochronous Transfer Packet Size Constraints:
            //
            // All device default interface settings must not include any
            // isochronous endpoints with non-zero data payload sizes (specified
            // via wMaxPacketSize in the endpoint descriptor)
            let issue = iface_descriptor
                .endpoint_descriptors()
                .find(|ep| ep.transfer_type() == TransferType::Isochronous
                    && ep.max_packet_size() != 0);
            if let Some(ep) = issue {
                panic!("Endpoint {} is isochronous and in the default setting",
                    ep.number());
            }
        } else {
            iso_ep_count += iface_descriptor.endpoint_descriptors()
                .filter(|ep| ep.transfer_type() == TransferType::Isochronous)
                .count();
        }
    }
    assert!(iso_ep_count > 0, "At least one isochronous endpoint is expected");
}

fn bulk_loopback(dev, _out) {
    let mut lens = vec![0, 1, 2, 32, 63, 64, 65, 127, 128, 129];
    if dev.is_high_speed() {
        lens.extend([255, 256, 257, 511, 512, 513, 1023, 1024, 1025]);
    }

    let max_packet_size: usize = dev.bulk_max_packet_size().into();
    for len in &lens {
        let data = random_data(*len);

        assert_eq!(
            dev.write_bulk(0x01, &data, TIMEOUT)
                .unwrap_or_else(|_| panic!("bulk write len {}", len)),
            data.len(),
            "bulk write len {}", len);

        if *len > 0 && *len % max_packet_size == 0 {
            assert_eq!(
                dev.write_bulk(0x01, &[], TIMEOUT)
                    .expect("bulk write zero-length packet"),
                0,
                "bulk write zero-length packet");
        }

        // Prevent libusb from instantaneously reading an empty packet on Windows when
        // zero-sized buffer is passed.
        let mut response = vec![0u8; max(*len, 1)];

        assert_eq!(
            dev.read_bulk(0x81, &mut response, TIMEOUT)
                .unwrap_or_else(|_| panic!("bulk read len {}", len)),
            data.len(),
            "bulk read len {}", len);

        assert_eq!(&response[..*len], &data[..]);
    }
}

fn interrupt_loopback(dev, _out) {
    for len in &[0, 1, 2, 15, 31] {
        let data = random_data(*len);

        assert_eq!(
            dev.write_interrupt(0x02, &data, TIMEOUT)
                .unwrap_or_else(|_| panic!("interrupt write len {}", len)),
            data.len(),
            "interrupt write len {}", len);

        // Prevent libusb from instantaneously reading an empty packet on Windows when
        // zero-sized buffer is passed.
        let mut response = vec![0u8; max(*len, 1)];

        assert_eq!(
            dev.read_interrupt(0x82, &mut response, TIMEOUT)
                .unwrap_or_else(|_| panic!("interrupt read len {}", len)),
            data.len(),
            "interrupt read len {}", len);

        assert_eq!(&response[..*len], &data[..]);
    }
}

fn bench_bulk_write(dev, out) {
    run_bench(dev, out, |data| {
        assert_eq!(
            dev.write_bulk(0x01, data, BENCH_TIMEOUT)
                .expect("bulk write"),
            data.len(),
            "bulk write");
    });
}

fn bench_bulk_read(dev, out) {
    run_bench(dev, out, |data| {
        assert_eq!(
            dev.read_bulk(0x81, data, BENCH_TIMEOUT)
                .expect("bulk read"),
            data.len(),
            "bulk read");
    });
}

}

fn run_bench(dev: &DeviceHandles, out: &mut String, f: impl Fn(&mut [u8])) {
    const TRANSFER_BYTES: usize = 64 * 1024;
    const TRANSFERS: usize = 16;
    const TOTAL_BYTES: usize = TRANSFER_BYTES * TRANSFERS;

    dev.write_control(
        request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
        test_class::REQ_SET_BENCH_ENABLED,
        1,
        0,
        &[],
        TIMEOUT,
    )
    .expect("enable bench mode");

    let mut data = random_data(TRANSFER_BYTES);

    let start = Instant::now();

    for _ in 0..TRANSFERS {
        f(&mut data);
    }

    let elapsed = start.elapsed();
    let elapsed = elapsed.as_secs() as f64 + (elapsed.subsec_micros() as f64) * 0.000_001;
    let throughput = (TOTAL_BYTES * 8) as f64 / 1_000_000.0 / elapsed;

    writeln!(
        out,
        "  {} transfers of {} bytes in {:.3}s -> {:.3}Mbit/s",
        TRANSFERS, TRANSFER_BYTES, elapsed, throughput
    )
    .expect("write failed");
}

fn random_data(len: usize) -> Vec<u8> {
    let mut data = vec![0u8; len];
    rand::thread_rng().fill(data.as_mut_slice());
    data
}
