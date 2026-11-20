//! `retention`'s protected records, in base: what keeps a record not had from outside the catalogue as the user data and the
//! data folder's stores name it, and the set as Discover reads it: the app's (`Serve::protected`, Soulseek's list among it)
//! where an app is made, else this part alone. `retention::protected` is this part and Soulseek's list.

use std::collections::BTreeSet;

use anyhow::Result;

use crate::serve::Serve;
use crate::userdata::UserData;

/// What keeps a record not had but Soulseek's list (`retention::protected`): every request's record, whatever its status
/// (`UserData::request_records`), anything the user data names it in (`UserData::records_named`) and the stores in the data
/// folder `data` (`stores_name`). Blocking.
pub fn protected_here(ud: &UserData, data: Option<&std::path::Path>) -> Result<BTreeSet<i64>> {
    let mut kept: BTreeSet<i64> = ud.request_records()?.into_iter().filter_map(|(_, record)| record).collect();
    kept.extend(ud.records_named()?);
    kept.extend(stores_name(data, |text| crate::serve::ids::records_named(text).collect())?);
    Ok(kept)
}

/// What keeps a record not had, as Cratarr's own library `s` is asked it: the app's word where it is made
/// (`Serve::protected`), else `protected_here`. Blocking.
pub fn protected_of(s: &Serve) -> Result<BTreeSet<i64>> {
    match s.protected() {
        Some(p) => p.protected_records(s),
        None => protected_here(&s.userdata, s.data_dir.as_deref()),
    }
}

/// What the stores in the data folder `data` name (`library::id_stores`: every account's crates, their seeds and conditions; the
/// queues), each read by `read`. A store not made yet names nothing; one that cannot be read may name anything, so nothing is
/// forgotten this time. Blocking.
pub fn stores_name(data: Option<&std::path::Path>, read: fn(&str) -> Vec<i64>) -> Result<BTreeSet<i64>> {
    let mut named = BTreeSet::new();
    for store in data.map(super::library::id_stores).unwrap_or_default() {
        match std::fs::read_to_string(&store) {
            Ok(text) => named.extend(read(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(anyhow::Error::from(e).context(format!("{} cannot be read", store.display()))),
        }
    }
    Ok(named)
}
