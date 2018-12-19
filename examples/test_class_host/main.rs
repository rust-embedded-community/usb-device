/// Runs tests against a TestClass device running on actual hardware using libusb.
///
/// This is implemented as an example as opposed to a test because the Rust test runner system is
/// not well suited for running tests that must depend on outside resources such as hardware and
/// cannot be run in parallel.

mod tests;
mod device;

use std::thread;
use std::time::Duration;
use std::panic;
use libusb::*;
use usb_device::device::CONFIGURATION_VALUE;
use crate::device::open_device;
use crate::tests::{TestFn, get_tests};

fn main() {
    let tests = get_tests();
    run_tests(&tests[..]);
}

fn run_tests(tests: &[(&str, TestFn)]) {
    const INTERFACE: u8 = 0;

    println!("testing with the TestClass");
    println!("looking for device...");

    let ctx = Context::new().expect("create libusb context");

    // Look for the device for about 5 seconds in case it hasn't finished enumerating yet
    let mut dev = Err(libusb::Error::NoDevice);
    for _ in 0..50 {
        dev = open_device(&ctx);
        if dev.is_ok() {
            break;
        }

        thread::sleep(Duration::from_millis(100));
    }

    let mut dev = match dev {
        Ok(d) => d,
        Err(err) => {
            println!("Did not find a TestClass device. Make sure the device is correctly programmed and plugged in. Last error: {}", err);
            return;
        }
    };

    println!("\nrunning {} tests", tests.len());

    let mut success = 0;
    for (name, test) in tests {
        if let Err(err) = dev.reset() {
            println!("Failed to reset the device: {}", err);
            return;
        }

        if let Err(err) = dev.set_active_configuration(CONFIGURATION_VALUE) {
            println!("Failed to set active configuration: {}", err);
            return;
        }

        if let Err(err) = dev.claim_interface(INTERFACE) {
            println!("Failed to claim interface: {}", err);
            return;
        }

        print!("test {} ... ", name);

        let hook = panic::take_hook();
        panic::set_hook(Box::new(|_| { }));
        let res = panic::catch_unwind(|| {
            test(&dev);
        });
        panic::set_hook(hook);

        dev.release_interface(INTERFACE).unwrap();

        if let Err(err) = res {
            let err = if let Some(err) = err.downcast_ref::<&'static str>() {
                String::from(*err)
            } else if let Some(err) = err.downcast_ref::<String>() {
                err.clone()
            } else {
                String::from("???")
            };

            println!("FAILED\nerror: {}\n", err);
        } else {
            println!("ok");
            success += 1;
        }
    }

    println!("{} failed, {} succeeded", tests.len() - success, success);

    if success == tests.len() {
        println!("\nALL TESTS PASSED!");
    }
}