//! The analysis pass's mode and whether this server hears beats, below Incoming (moved from `app::analysis_pass`, which keeps
//! their names): the server's settings in force hold them (`runtime_base`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Keep,
    All,
    Off,
}

impl Mode {
    pub fn from_env(v: Option<&str>) -> Self {
        match v.map(|v| v.trim().to_lowercase()).as_deref() {
            Some("all") => Mode::All,
            Some("off") => Mode::Off,
            _ => Mode::Keep,
        }
    }
}

/// Whether this server hears new music's beats itself (`CRATARR_BEAT_THIS`: `on`; off by default, see the top).
pub fn hears_beats(v: Option<&str>) -> bool {
    v.is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "on" | "1" | "true" | "yes"))
}
