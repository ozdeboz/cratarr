//! Cratarr's rules for the Android app (Spike B, 0.11; the owner, 2026-10-30): the rules crate as the app's Kotlin calls
//! it, through UniFFI, so the app, the page (WebAssembly) and the server share one implementation. Behind the app's
//! `rustRules` switch, off by default; the app's own Kotlin stays until the spike is judged. Only the brake so far, as
//! the page's first WebAssembly step.

uniffi::setup_scaffolding!();

/// How long a brake from `v0` to `to` lasts (ms), never shorter than `brake_ms`.
#[uniffi::export]
pub fn brake_length_ms(v0: f64, to: f64, brake_ms: f64) -> f64 {
    cratarr_rules::brake::length_ms(v0, to, brake_ms)
}

/// Its speed `t` ms after it was let go (×).
#[uniffi::export]
pub fn brake_v(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    cratarr_rules::brake::v(v0, to, length, t)
}

/// Its way by `t` ms after it was let go (ms of track).
#[uniffi::export]
pub fn brake_way(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    cratarr_rules::brake::way(v0, to, length, t)
}

/// How fast its speed changes `t` ms after it was let go (× a second).
#[uniffi::export]
pub fn brake_accel(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    cratarr_rules::brake::accel(v0, to, length, t)
}
