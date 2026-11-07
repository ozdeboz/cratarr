//! The page's way in (the 0075 spike, the owner 2026-10-30): plain exports, no bindings to generate, no imports. Three
//! modules from this one crate: `rules.wasm`, the brake (feature `wasm-brake`), small and loaded with the page;
//! `matcher.wasm`, the matcher (feature `wasm-matcher`), fetched when the page is idle; `index.wasm`, the library on the
//! device (feature `wasm-index`), fetched when the page first syncs or reads it (scripts/rules-wasm.sh).

#[cfg(feature = "wasm-brake")]
#[unsafe(no_mangle)]
pub extern "C" fn brake_length_ms(v0: f64, to: f64, brake_ms: f64) -> f64 {
    crate::brake::length_ms(v0, to, brake_ms)
}

#[cfg(feature = "wasm-brake")]
#[unsafe(no_mangle)]
pub extern "C" fn brake_v(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::v(v0, to, length, t)
}

#[cfg(feature = "wasm-brake")]
#[unsafe(no_mangle)]
pub extern "C" fn brake_way(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::way(v0, to, length, t)
}

#[cfg(feature = "wasm-brake")]
#[unsafe(no_mangle)]
pub extern "C" fn brake_accel(v0: f64, to: f64, length: f64, t: f64) -> f64 {
    crate::brake::accel(v0, to, length, t)
}

#[cfg(feature = "wasm-matcher")]
mod matcher_exports {
    // The matcher (0.12: one matcher, the owner 2026-10-30): JSON over the module's memory, no bindings. The page asks for
    // room (`matcher_alloc`), writes UTF-8 JSON there, and frees it after the call (`matcher_free`, the same length); an
    // index lives here under its handle until `matcher_drop`. A query's answer is room the page frees: a little-endian u32
    // length, then that many bytes of UTF-8 JSON (`matcher_free(ptr, 4 + length)`).

    use crate::matcher::{Index, json};
    use std::sync::{Mutex, MutexGuard};

    /// The indexes made, by handle − 1; a dropped one leaves its place free.
    static INDEXES: Mutex<Vec<Option<Index>>> = Mutex::new(Vec::new());

    fn indexes() -> MutexGuard<'static, Vec<Option<Index>>> {
        INDEXES.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The bytes the page wrote at `ptr` (`len` of them).
    unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
        if ptr.is_null() || len == 0 { &[] } else { unsafe { core::slice::from_raw_parts(ptr, len) } }
    }

    /// Room for `len` bytes, zeroed.
    #[unsafe(no_mangle)]
    pub extern "C" fn matcher_alloc(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
    }

    /// Frees room from `matcher_alloc` or a query's answer: its pointer and its whole length.
    ///
    /// # Safety
    /// `ptr` and `len` as given out here, once.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn matcher_free(ptr: *mut u8, len: usize) {
        if !ptr.is_null() {
            drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) });
        }
    }

    /// An index of `{names:[{text, aliases?}]}`: its handle (from 1), or 0 for JSON not of that shape.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn matcher_index(ptr: *const u8, len: usize) -> u32 {
        let Some(names) = json::parse(unsafe { input(ptr, len) }).as_ref().and_then(json::names_of) else {
            return 0;
        };
        let index = Index::new(names);
        let mut all = indexes();
        let slot = match all.iter().position(Option::is_none) {
            Some(free) => {
                all[free] = Some(index);
                free
            }
            None => {
                all.push(Some(index));
                all.len() - 1
            }
        };
        slot as u32 + 1
    }

    /// A query `{q, limit}` asked of an index: its hits `[{i, score, how}]`, best first, behind their length (see above);
    /// null for an unknown handle or input that is not JSON.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn matcher_query(handle: u32, ptr: *const u8, len: usize) -> *mut u8 {
        let Some(ask) = json::parse(unsafe { input(ptr, len) }) else {
            return core::ptr::null_mut();
        };
        let (q, limit) = json::ask_of(&ask);
        let all = indexes();
        let Some(Some(index)) = (handle as usize).checked_sub(1).and_then(|i| all.get(i)) else {
            return core::ptr::null_mut();
        };
        let body = json::hits(&index.query(&q, limit));
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body.as_bytes());
        Box::into_raw(out.into_boxed_slice()) as *mut u8
    }

    /// Lets an index go.
    #[unsafe(no_mangle)]
    pub extern "C" fn matcher_drop(handle: u32) {
        let mut all = indexes();
        if let Some(slot) = (handle as usize).checked_sub(1).and_then(|i| all.get_mut(i)) {
            *slot = None;
        }
    }
}

// The library on a device (PREVIEW2's D14): bytes in and out over the module's memory, as the matcher's are. An instance holds one
// index and one set of sounds at most, and neither unless asked: WebAssembly's memory never shrinks, so the page lets a whole
// instance go to give it back. `facts`, `Alike` and Radio's index are not made here until a crate or a pick asks (D18 to D20).
#[cfg(feature = "wasm-index")]
mod index_exports {
    use crate::index::sync::{ANEW_AFTER_MS, FIRST_AFTER_MS, Mobile, Network, Outcome, Part, Step};
    use crate::index::{Changes, Index, Refused, SOUNDS_VERSION, SoundChanges, Sounds, VERSION};
    use std::sync::{Mutex, MutexGuard};

    static INDEX: Mutex<Option<Index>> = Mutex::new(None);
    static SOUNDS: Mutex<Option<Sounds>> = Mutex::new(None);
    /// The version the last refusal found, and the one this module reads.
    static VERSIONS: Mutex<(u32, u32)> = Mutex::new((0, 0));

    fn held<T>(slot: &'static Mutex<T>) -> MutexGuard<'static, T> {
        slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The bytes the page wrote at `ptr` (`len` of them).
    unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
        if ptr.is_null() || len == 0 { &[] } else { unsafe { core::slice::from_raw_parts(ptr, len) } }
    }

    /// What the page frees, as a query's answer is: a little-endian u32 length, then that many bytes.
    fn answer(body: &[u8]) -> *mut u8 {
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        Box::into_raw(out.into_boxed_slice()) as *mut u8
    }

    /// 0: taken. Else why not: 1 another version (`refused_found`, `refused_wants`), 2 damaged, 3 not where it stands, 4 naming
    /// what it does not hold. What was held stays as it was.
    fn said(r: Result<(), Refused>) -> u32 {
        match r {
            Ok(()) => 0,
            Err(Refused::Version { found, wants }) => {
                *held(&VERSIONS) = (found, wants);
                1
            }
            Err(Refused::Damaged) => 2,
            Err(Refused::Rev { .. }) => 3,
            Err(Refused::Unknown { .. }) => 4,
        }
    }

    /// Room for `len` bytes, zeroed.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_alloc(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
    }

    /// Frees room from `index_alloc` or an answer: its pointer and its whole length.
    ///
    /// # Safety
    /// `ptr` and `len` as given out here, once.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_free(ptr: *mut u8, len: usize) {
        if !ptr.is_null() {
            drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) });
        }
    }

    /// The version this module reads, of an index and of the sounds.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_version() -> u32 {
        VERSION
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn sounds_version() -> u32 {
        SOUNDS_VERSION
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn refused_found() -> u32 {
        held(&VERSIONS).0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn refused_wants() -> u32 {
        held(&VERSIONS).1
    }

    /// An index from its bytes: `said`'s answer.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_load(ptr: *const u8, len: usize) -> u32 {
        said(Index::load(unsafe { input(ptr, len) }).map(|index| *held(&INDEX) = Some(index)))
    }

    /// The feed's bytes applied to the index held: `said`'s answer.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_apply(ptr: *const u8, len: usize) -> u32 {
        let changes = match Changes::load(unsafe { input(ptr, len) }) {
            Ok(changes) => changes,
            Err(why) => return said(Err(why)),
        };
        match held(&INDEX).as_mut() {
            Some(index) => said(index.apply(&changes)),
            None => 3,
        }
    }

    /// The index held, as bytes; null where none is.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_save() -> *mut u8 {
        held(&INDEX).as_ref().map_or(core::ptr::null_mut(), |index| answer(&index.save()))
    }

    /// Where the index held stands; -1 where none is.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_rev() -> f64 {
        held(&INDEX).as_ref().map_or(-1.0, |index| index.rev() as f64)
    }

    /// How many tracks it holds.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_tracks() -> u32 {
        held(&INDEX).as_ref().map_or(0, |index| index.tracks() as u32)
    }

    /// Every library and record, as JSON (`Index::list_json`); null where no index is held.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_list() -> *mut u8 {
        held(&INDEX).as_ref().map_or(core::ptr::null_mut(), |index| answer(index.list_json().as_bytes()))
    }

    /// A record and its tracks, as JSON (`Index::record_json`); null where it is not held.
    #[unsafe(no_mangle)]
    pub extern "C" fn index_record(id: f64) -> *mut u8 {
        held(&INDEX).as_ref().and_then(|index| index.record_json(id as i64)).map_or(core::ptr::null_mut(), |json| answer(json.as_bytes()))
    }

    /// The sounds from their bytes: `said`'s answer.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sounds_load(ptr: *const u8, len: usize) -> u32 {
        said(Sounds::load(unsafe { input(ptr, len) }).map(|sounds| *held(&SOUNDS) = Some(sounds)))
    }

    /// The feed's bytes applied to the sounds held: `said`'s answer.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sounds_apply(ptr: *const u8, len: usize) -> u32 {
        let changes = match SoundChanges::load(unsafe { input(ptr, len) }) {
            Ok(changes) => changes,
            Err(why) => return said(Err(why)),
        };
        match held(&SOUNDS).as_mut() {
            Some(sounds) => said(sounds.apply(&changes)),
            None => 3,
        }
    }

    /// The sounds held, as bytes; null where none are.
    #[unsafe(no_mangle)]
    pub extern "C" fn sounds_save() -> *mut u8 {
        held(&SOUNDS).as_ref().map_or(core::ptr::null_mut(), |sounds| answer(&sounds.save()))
    }

    /// Where the sounds held stand; -1 where none are.
    #[unsafe(no_mangle)]
    pub extern "C" fn sounds_rev() -> f64 {
        held(&SOUNDS).as_ref().map_or(-1.0, |sounds| sounds.rev() as f64)
    }

    /// `index::sync`'s state from the page's numbers: `tried` below zero for none.
    fn state(whole: u32, stopped: u32, lost: u32, failed: u32, tried: f64, started: f64) -> Part {
        Part { whole: whole != 0, stopped: stopped != 0, lost, failed, tried: (tried >= 0.0).then_some(tried as i64), started: started as i64 }
    }

    /// What to do now (`Part::step`). `network`: 0 free, 1 metered, 2 off; `mobile`: 0 unasked, 1 yes, 2 no. Its answer: a time to wait
    /// for (ms), or below zero: -1 stopped, -2 rest, -3 ask, -4 whole, -5 changes.
    #[unsafe(no_mangle)]
    #[allow(clippy::too_many_arguments)]
    pub extern "C" fn sync_step(whole: u32, stopped: u32, lost: u32, failed: u32, tried: f64, started: f64, network: u32, mobile: u32, now: f64) -> f64 {
        let network = [Network::Free, Network::Metered][..].get(network as usize).copied().unwrap_or(Network::Off);
        let mobile = [Mobile::Unasked, Mobile::Yes][..].get(mobile as usize).copied().unwrap_or(Mobile::No);
        match state(whole, stopped, lost, failed, tried, started).step(network, mobile, now as i64) {
            Step::Wait(until) => until as f64,
            Step::Stopped => -1.0,
            Step::Rest => -2.0,
            Step::Ask => -3.0,
            Step::Whole => -4.0,
            Step::Changes => -5.0,
        }
    }

    /// How long after a start the first fetch waits (ms): the page looks then, not to load this module sooner.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_first_after() -> f64 {
        FIRST_AFTER_MS as f64
    }

    /// Hidden this long (ms), a device coming back has started again.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_anew_after() -> f64 {
        ANEW_AFTER_MS as f64
    }

    /// Whether a part lost this many times in a row says it is paused.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_paused(lost: u32) -> u32 {
        u32::from(Part { lost, ..Part::default() }.paused())
    }

    /// How a part stands after a fetch (`Part::after`), or at a start (`Part::anew`). `outcome`: 0 took, 1 lost, 2 failed, 3 start again, 4
    /// another version, 5 a start. Its answer in bits: 1 whole, 2 stopped, then lost from bit 8 and failed from bit 16.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_after(whole: u32, stopped: u32, lost: u32, failed: u32, outcome: u32) -> u32 {
        let outcome = [Outcome::Took, Outcome::Lost, Outcome::Failed, Outcome::StartAgain, Outcome::Version][..].get(outcome as usize).copied();
        let s = state(whole, stopped, lost, failed, -1.0, 0.0);
        let s = outcome.map_or(s.anew(0), |o| s.after(o, 0));
        u32::from(s.whole) | u32::from(s.stopped) << 1 | s.lost.min(255) << 8 | s.failed.min(255) << 16
    }
}

// A crate's records kept apart (0.13, one spread: the page's own copy goes): numbers in and out, the page giving each
// record a small number. `spread_alloc` room for `n` of them, `spread_order` the places in their play order (room the
// page frees with `spread_free`, as it frees its own).
#[cfg(feature = "wasm-spread")]
mod spread_exports {
    /// Room for `n` numbers (u32), zeroed.
    #[unsafe(no_mangle)]
    pub extern "C" fn spread_alloc(n: u32) -> *mut u32 {
        Box::into_raw(vec![0u32; n as usize].into_boxed_slice()).cast::<u32>()
    }

    /// Frees room from `spread_alloc` or an order from `spread_order`: its pointer and its length.
    ///
    /// # Safety
    /// `ptr` and `n` as given out here, once.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn spread_free(ptr: *mut u32, n: u32) {
        if !ptr.is_null() {
            drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, n as usize)) });
        }
    }

    /// `n` records' keys at `keys` in their order: their places in play order (`crate::spread`), `n` of them.
    ///
    /// # Safety
    /// `keys` holds `n` numbers (room from `spread_alloc`).
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn spread_order(keys: *const u32, n: u32, gap: u32) -> *mut u32 {
        let keys = unsafe { std::slice::from_raw_parts(keys, n as usize) };
        let order: Vec<u32> = crate::spread(keys, gap as usize).into_iter().map(|i| i as u32).collect();
        Box::into_raw(order.into_boxed_slice()).cast::<u32>()
    }
}

