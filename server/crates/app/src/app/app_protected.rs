//! What keeps a record not had, as the app knows it (`serve::protected::Protected`): `retention::protected`, Soulseek's list
//! among it.

use std::collections::BTreeSet;

use anyhow::Result;

use crate::serve::Serve;
use crate::serve::protected::Protected;

pub(super) struct AppProtected;

impl Protected for AppProtected {
    fn protected_records(&self, serve: &Serve) -> Result<BTreeSet<i64>> {
        super::retention::protected(&serve.userdata, serve.data_dir.as_deref())
    }
}
