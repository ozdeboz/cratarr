//! The backup's list of shared copies written (C41), below the app (moved from `app::backup_list`, which keeps their names):
//! the server writes it anew at the end of every library build (`Serve::refresh`).

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::serve::Serve;

/// The list's file, in the data folder.
pub const LIST: &str = "backup-shared-copies.txt";

/// The paths of the shared copies outside catalogue folder `main` (`Catalogue::shared_copies`) of an original kept in one of the
/// folders `kept`, each as Cratarr sees it. Blocking.
pub fn paths(cat: &Catalogue, main: i64, kept: &[i64]) -> Result<Vec<PathBuf>> {
    let folders = cat.folders();
    Ok(cat.shared_copies(main, kept)?.into_iter().filter_map(|(folder, rel)| Some(folders.get(folder)?.join(rel))).collect())
}

/// The list written anew in `data`: through a part of this writer's own (`part_in`), made sure of on disk, then renamed, so two
/// writes at once (a copy's refresh and a build's) each leave a whole list, and a crash leaves the old one or the new (T12). On
/// any error the part goes and the old list stays. How many copies. Blocking.
pub fn write(cat: &Catalogue, main: i64, kept: &[i64], data: &Path) -> Result<usize> {
    use std::io::Write;
    let paths = paths(cat, main, kept)?;
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    let (to, part) = (data.join(LIST), part_in(data));
    let written = (|| -> Result<()> {
        let mut file = std::fs::File::create(&part)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(std::fs::rename(&part, &to)?)
    })();
    if written.is_err() && part.is_file() {
        std::fs::remove_file(&part).ok();
    }
    written.map(|()| paths.len())
}

/// A write's part in `data`: the process's and the writing thread's, so two writes at once never share one (T12).
pub fn part_in(data: &Path) -> PathBuf {
    let thread: String = format!("{:?}", std::thread::current().id()).chars().filter(char::is_ascii_digit).collect();
    data.join(format!("{LIST}.{}-{thread}.part", std::process::id()))
}

/// The list written anew for the server (`served`): Music the first main library; the main libraries and anyone's own music
/// kept. Nothing where there is no data folder or no main library. Blocking.
pub fn write_for(served: &Serve) -> Result<usize> {
    use crate::settings::LibraryKind;
    let kinds = served.library.kinds();
    let (Some(data), Some(main)) = (served.data_dir.as_deref(), kinds.iter().find(|(_, k)| *k == LibraryKind::Main).map(|(f, _)| *f)) else {
        return Ok(0);
    };
    let kept: Vec<i64> = kinds.iter().filter(|(_, k)| *k != LibraryKind::Incoming).map(|(f, _)| *f).collect();
    write(served.catalogue(), main, &kept, data)
}
