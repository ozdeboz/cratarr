//! The plugins, listed (decision 0005): every kind of source compiled in, and one found by its id. Apart from `source`'s
//! types, which the stores below the plugins read (SPLIT-PLAN P2).

use super::{Kind, cratarr, jellyfin, subsonic};

pub static KINDS: &[Kind] = &[cratarr::KIND, jellyfin::KIND, subsonic::KIND];

pub fn kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FieldKind, Values};

    /// The contract every plugin must keep; a new plugin gets these checks for free.
    #[test]
    fn every_plugin_describes_its_fields_consistently() {
        let mut ids = std::collections::HashSet::new();
        for k in KINDS {
            assert!(ids.insert(k.id), "duplicate plugin id {}", k.id);
            assert!(!k.id.is_empty() && k.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            let fields = (k.fields)();
            let keys: Vec<&str> = fields.iter().map(|f| f.key).collect();
            for f in &fields {
                assert_eq!(keys.iter().filter(|x| **x == f.key).count(), 1, "{}: duplicate field {}", k.id, f.key);
                assert!(
                    f.bound_to.is_empty() || f.kind == FieldKind::Secret,
                    "{}: only secrets can be bound ({})",
                    k.id,
                    f.key
                );
                for b in f.bound_to {
                    assert!(keys.contains(b), "{}: {} is bound to unknown field {b}", k.id, f.key);
                }
            }
            // With nothing filled in, a plugin that needs settings must refuse cleanly rather than panic.
            if fields.iter().any(|f| f.required) {
                assert!((k.connect)(&Values::new(), false).is_err());
                assert!((k.connect)(&Values::new(), true).is_err());
            }
        }
    }
}
