//! What keeps a record not had from being forgotten, as the app knows it (`app::retention::protected`, Soulseek's list among
//! it): set on Serve as the app is made (`Serve::set_protected`), as what the crates' facts take from it is
//! (`services::FactsFrom`).

use std::collections::BTreeSet;

use anyhow::Result;

use super::Serve;

pub trait Protected: Send + Sync {
    /// The records nothing may forget (`app::retention::protected`). Blocking.
    fn protected_records(&self, serve: &Serve) -> Result<BTreeSet<i64>>;
}
