/// Runs tests against a TestClass device running on actual hardware using libusb.
///
/// This is implemented as an example as opposed to a test because the Rust test runner system is
/// not well suited for running tests that must depend on outside resources such as hardware and
/// cannot be run in parallel..

mod tests;
mod device;

use std::panic;
use libusb::*;
use crate::device::open_device;
use crate::tests::{TestFn, get_tests};

fn main() {
    let tests = get_tests();
    run_tests(&tests[..]);
}

fn run_tests(tests: &[(&str, TestFn)]) {
    println!("testing usb-device hardware");

    let ctx = Context::new().expect("create libusb context");
    let mut dev = match open_device(&ctx) {
        Some(d) => d,
        None => {
            println!("Did not find a TestClass device. Make sure the device is correctly programmed and plugged in.");
            return;
        }
    };

    println!("running {} tests", tests.len());

    let mut success = 0;
    for (name, test) in tests {
        if let Err(err) = dev.reset() {
            println!("Failed to reset the device: {}", err);
            return;
        }

        if let Err(err) = dev.set_active_configuration(1) {
            println!("Failed to set active configuration: {}", err);
            return;
        }

        print!("test {} ... ", name);

        let hook = panic::take_hook();
        panic::set_hook(Box::new(|_| { }));
        let res = panic::catch_unwind(|| {
            test(&dev);
        });
        panic::set_hook(hook);

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