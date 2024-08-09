mod device;
/// Runs tests against a TestClass device running on actual hardware using libusb.
///
/// This is implemented as an example as opposed to a test because the Rust test runner system is
/// not well suited for running tests that must depend on outside resources such as hardware and
/// cannot be run in parallel.
mod tests;

use crate::device::UsbContext;
use crate::tests::{get_tests, TestFn};
use std::io::prelude::*;
use std::io::stdout;
use std::panic;

fn main() {
    let tests = get_tests();
    run_tests(&tests[..]);
}

fn run_tests(tests: &[(&str, TestFn)]) {
    println!("test_class_host starting");
    println!("looking for device...");

    let mut ctx = UsbContext::new().expect("create libusb context");

    println!("\nrunning {} tests", tests.len());

    let mut success = 0;
    for (name, test) in tests {
        print!("test {} ... ", name);
        let _ = stdout().flush();

        let mut out = String::new();

        let res = {
            let hook = panic::take_hook();
            panic::set_hook(Box::new(|_| {}));
            let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                test(&mut ctx, &mut out);
            }));
            panic::set_hook(hook);

            res
        };

        if let Err(err) = ctx.cleanup_after_test() {
            println!("Failed to release interface: {}", err);
            panic!("post test cleanup failed");
        }

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

            if !out.is_empty() {
                print!("{}", out);
            }

            success += 1;
        }
    }

    println!("{} failed, {} succeeded", tests.len() - success, success);

    if success == tests.len() {
        println!("\nALL TESTS PASSED!");
    } else {
        std::process::exit(1);
    }
}
