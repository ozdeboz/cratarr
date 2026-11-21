//! Cratarr's rules for the Android app (Spike B, 0.11; the owner, 2026-10-30): the rules crate as the app's Kotlin calls
//! it, through UniFFI, so the app, the page (WebAssembly) and the server share one implementation. In every build of the
//! app (`rulesLibrary` in its Gradle build). The brake, what the app's sync of the library asks, and what a device answers for itself.

uniffi::setup_scaffolding!();

mod device;
mod device_index;
mod fresh;

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

/// A record's artists as one text where only text fits, the notification's (NAMES.24's one formatter: "A & B", "A, B & C", or a
/// confirming source's own phrase), as the server and the page say it.
#[uniffi::export]
pub fn credit_text(names: Vec<String>, phrase: Option<String>) -> String {
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    cratarr_rules::names::credit_text(&names, phrase.as_deref())
}
