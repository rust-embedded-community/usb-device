#[cfg(all(feature = "log", not(feature = "defmt")))]
macro_rules! usb_log {
    (trace, $($arg:expr),*) => { log::trace!($($arg),*) };
    (debug, $($arg:expr),*) => { log::debug!($($arg),*) };
}

#[cfg(feature = "defmt")]
macro_rules! usb_log {
    (trace, $($arg:expr),*) => { defmt::trace!($($arg),*) };
    (debug, $($arg:expr),*) => { defmt::debug!($($arg),*) };
}

#[cfg(not(any(feature = "log", feature = "defmt")))]
macro_rules! usb_log {
    ($level:ident, $($arg:expr),*) => {{ $( let _ = $arg; )* }}
}

macro_rules! usb_trace {
    ($($arg:expr),*) => (usb_log!(trace, $($arg),*));
}

macro_rules! usb_debug {
    ($($arg:expr),*) => (usb_log!(debug, $($arg),*));
}
