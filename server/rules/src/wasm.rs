//! The page's way in (the 0075 spike, the owner 2026-10-30): plain exports of numbers, no bindings to generate, no
//! imports. The page instantiates the module once and calls these as the app calls its own.

#[unsafe(no_mangle)]
pub extern "C" fn brake_length_ms(v0: f64, to: f64, brake_ms: f64) -> f64 {
    crate::brake::length_ms(v0, to, brake_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn brake_v(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::v(v0, to, length, t)
}

#[unsafe(no_mangle)]
pub extern "C" fn brake_way(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::way(v0, to, length, t)
}

#[unsafe(no_mangle)]
pub extern "C" fn brake_accel(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::accel(v0, to, length, t)
}
