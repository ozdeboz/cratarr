//! What the turn reads of requests.rs, moved down from the app (its HTTP dates).

/// The day a release date names ("2026", "2026-12", "2026-12-04"), at its start, epoch seconds (UTC). Pure.
pub fn release_day(s: &str) -> Option<i64> {
    let mut parts = s.trim().split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 1,
    };
    let d: i64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 1,
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from 1970-01-01 (days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}
