//! `file_facts`' switch, in base: whether types and years are written into the files (decision 0097), read and set
//! by the words' Catalogue sheet as by Incoming's writer. `file_facts` re-exports it.

use anyhow::Result;

/// The server choice that switches it on ("1").
pub const CHOICE: &str = "write:file-facts";

/// On unless switched off: type and year write by default (decision 0097, the Catalogue sheet's Write).
pub fn is_on(serve: &crate::serve::Serve) -> bool {
    serve.userdata.import_choice(CHOICE).ok().flatten().is_none_or(|c| c == "1")
}

pub fn set_on(serve: &crate::serve::Serve, on: bool) -> Result<()> {
    serve.userdata.set_import_choice(CHOICE, if on { "1" } else { "0" })
}
