//! A let-go's brake (decision 0075's one plan; the owner, 2026-10-29: "When spinning down from high speeds, braking
//! takes longer"): from `v0` × down to `to` × over its length, a raised cosine; its length at a constant mean
//! deceleration of [`DECEL_X`] × a second, never shorter than the Brake as set. Times in ms, speeds in ×, way in ms of
//! track. shared/rules.json holds worked cases every player checks against.

use core::f64::consts::PI;

/// A brake's mean deceleration (× a second): 128× stops in 2 s, 32× in 0.5 s.
pub const DECEL_X: f64 = 64.0;

/// How long a brake from `v0` to `to` lasts (ms): at [`DECEL_X`], never shorter than `brake_ms` (the Brake as set).
pub fn length_ms(v0: f64, to: f64, brake_ms: f64) -> f64 {
    brake_ms.max(((v0 - to).abs() / DECEL_X * 1000.0).round())
}

/// Its speed `t` ms after it was let go (×).
pub fn v(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    if t >= length {
        return to;
    }
    to + (v0 - to) * (1.0 + (PI * t.max(0.0) / length).cos()) / 2.0
}

/// Its way by `t` ms after it was let go (ms of track).
pub fn way(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    let s = t.max(0.0).min(length);
    to * s + (v0 - to) * (s / 2.0 + length / (2.0 * PI) * (PI * s / length).sin()) + to * (t - length).max(0.0)
}

/// How fast its speed changes `t` ms after it was let go (× a second).
pub fn accel(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    if t < 0.0 || t >= length {
        return 0.0;
    }
    -(v0 - to) * PI / (2.0 * length) * (PI * t / length).sin() * 1000.0
}
