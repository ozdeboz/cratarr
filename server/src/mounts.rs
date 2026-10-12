//! The folders mounted into Cratarr's container, offered in Settings as music folders (decision 0009): read from
//! /proc/self/mountinfo, keeping data mounts and leaving out the system ones (/, /proc, /sys, /dev, /etc/hosts…)
//! and Cratarr's own data folder. Mount points only, not what is inside them.

/// File systems that never hold music.
const SYSTEM_FS: [&str; 16] = [
    "proc", "sysfs", "tmpfs", "devpts", "mqueue", "cgroup", "cgroup2", "overlay", "devtmpfs", "securityfs", "debugfs",
    "tracefs", "nsfs", "autofs", "binfmt_misc", "pstore",
];
const SYSTEM_DIRS: [&str; 6] = ["/proc", "/sys", "/dev", "/etc", "/run", "/tmp"];

/// Mount points from a mountinfo text, sorted, without system mounts or `exclude` (the data folder).
pub fn parse(mountinfo: &str, exclude: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = unescape(left.split_whitespace().nth(4)?);
            let fstype = right.split_whitespace().next()?;
            let system = point == "/"
                || SYSTEM_FS.contains(&fstype)
                || SYSTEM_DIRS.iter().any(|d| point == *d || point.starts_with(&format!("{d}/")))
                || exclude.iter().any(|e| point == e.trim_end_matches('/'));
            (!system).then_some(point)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// mountinfo escapes space, tab, newline and backslash as octal (\040 …).
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The folders mounted into this process's container, as `parse` describes; empty when the table cannot be read.
pub fn list(exclude: &[&str]) -> Vec<String> {
    std::fs::read_to_string("/proc/self/mountinfo").map(|t| parse(&t, exclude)).unwrap_or_default()
}

/// Whether Cratarr can write in `dir`, tested by writing (as the *arr apps test a root folder): a read-only mount and
/// wrong ownership both fail it. Leaves nothing behind. Blocking.
pub fn writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(format!(".cratarr-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => std::fs::remove_file(&probe).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_folder_is_seen_as_writable() {
        let d = std::env::temp_dir().join(format!("cratarr-writable-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(writable(&d));
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0, "the test file is removed");
        assert!(!writable(&d.join("missing")));
        std::fs::remove_dir_all(&d).ok();
    }

    // Shaped like the reference install's cratarr container (docker on TOS).
    const INFO: &str = "\
1201 1100 0:84 / / rw,relatime master:443 - overlay overlay rw,lowerdir=/x
1202 1201 0:87 / /proc rw,nosuid - proc proc rw
1203 1201 0:88 / /dev rw,nosuid - tmpfs tmpfs rw
1206 1201 0:91 / /sys ro - sysfs sysfs ro
1210 1201 8:3 /Volume1/media/Music /music rw,relatime - btrfs /dev/md2 rw
1211 1201 8:3 /Volume1/docker/cratarr/data /data rw,relatime - btrfs /dev/md2 rw
1212 1201 8:3 /Volume1/media/lidarr-music /incoming ro,relatime - btrfs /dev/md2 rw
1213 1201 8:3 /Volume1/docker/etc/resolv.conf /etc/resolv.conf rw - btrfs /dev/md2 rw
1214 1201 8:3 /Volume1/media/Old\\040Stuff /old\\040stuff ro - btrfs /dev/md2 rw
";

    #[test]
    fn data_mounts_only() {
        assert_eq!(parse(INFO, &["/data"]), vec!["/incoming", "/music", "/old stuff"]);
        assert_eq!(parse(INFO, &[]), vec!["/data", "/incoming", "/music", "/old stuff"]);
    }

    #[test]
    fn junk_lines_are_ignored() {
        assert!(parse("nonsense\n\n1 2 3\n", &[]).is_empty());
    }
}
