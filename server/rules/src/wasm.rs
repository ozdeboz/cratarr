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
// instance go to give it back. `facts`, `Alike` and Radio's index are made only once something asks what a device answers for itself
// (`device_open`, `index::device`), and go with the instance.
#[cfg(feature = "wasm-index")]
mod index_exports {
    use crate::index::device::Device;
    use crate::index::find::{Ask, Finder};
    use crate::index::sync::{ANEW_AFTER_MS, COVERS_VERSION, FIRST_AFTER_MS, Mobile, Network, Outcome, Part, Step, covers_rev, ended};
    use crate::index::{Changes, Index, MARKS_VERSION, Marks, Refused, SOUNDS_VERSION, SoundChanges, Sounds, VERSION};
    use std::sync::{Arc, Mutex, MutexGuard};

    static INDEX: Mutex<Option<Arc<Index>>> = Mutex::new(None);
    static SOUNDS: Mutex<Option<Sounds>> = Mutex::new(None);
    /// What a search reads of the index held: made at the first search, let go with any change to it. Never by its rev: another
    /// account's stands at the same one.
    static FINDER: Mutex<Option<Finder>> = Mutex::new(None);
    static MARKS: Mutex<Option<Marks>> = Mutex::new(None);
    /// What the page asks of a device, once it was opened (`device_open`).
    static DEVICE: Mutex<Option<Device>> = Mutex::new(None);
    /// Where the covers last taken stand: their frames are the page's, and only this is the crate's.
    static COVERS: Mutex<f64> = Mutex::new(-1.0);
    /// The version the last refusal found, and the one this module reads.
    static VERSIONS: Mutex<(u32, u32)> = Mutex::new((0, 0));

    fn held<T>(slot: &'static Mutex<T>) -> MutexGuard<'static, T> {
        slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `index` is the one held from now on. What was made over the one before goes with it: its finder, a device opened over it (which
    /// would answer from what is no longer held, and keep it in memory), and the account's own loaded beside it, which a page loads
    /// again for the one now held (audit/OFFLINE-REVIEW.md, finding 5).
    fn hold(index: Index) {
        *held(&DEVICE) = None;
        *held(&MARKS) = None;
        *held(&INDEX) = Some(index.into());
        *held(&FINDER) = None;
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
        said(Index::load(unsafe { input(ptr, len) }).map(hold))
    }

    /// The feed's bytes applied to the index held: `said`'s answer. A device opened over it goes first, as over any other index held
    /// (`hold`): it would answer from the index before, and while it held it, applying would copy the whole index.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_apply(ptr: *const u8, len: usize) -> u32 {
        let changes = match Changes::load(unsafe { input(ptr, len) }) {
            Ok(changes) => changes,
            Err(why) => return said(Err(why)),
        };
        *held(&DEVICE) = None;
        match held(&INDEX).as_mut() {
            Some(index) => {
                *held(&FINDER) = None;
                said(Arc::make_mut(index).apply(&changes))
            }
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

    /// What a search finds, as JSON (`Index::found_json`), from an ask as JSON (`Ask::read`); null where no index is held, or the ask
    /// cannot be read.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_find(ptr: *const u8, len: usize) -> *mut u8 {
        let Some(ask) = Ask::read(unsafe { input(ptr, len) }) else {
            return core::ptr::null_mut();
        };
        let index = held(&INDEX);
        let Some(index) = index.as_ref() else {
            return core::ptr::null_mut();
        };
        let mut finder = held(&FINDER);
        answer(index.found_json(finder.get_or_insert_with(|| index.finder()), &ask).as_bytes())
    }

    /// An artist as their panel shows them, as JSON (`Index::artist_json`), from an ask naming them as its `q`; null where nobody goes by
    /// that name.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn index_artist(ptr: *const u8, len: usize) -> *mut u8 {
        let Some(ask) = Ask::read(unsafe { input(ptr, len) }) else {
            return core::ptr::null_mut();
        };
        held(&INDEX).as_ref().and_then(|index| index.artist_json(&ask.q, &ask.without)).map_or(core::ptr::null_mut(), |json| answer(json.as_bytes()))
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

    #[unsafe(no_mangle)]
    pub extern "C" fn marks_version() -> u32 {
        MARKS_VERSION
    }

    /// An account's own from its bytes: `said`'s answer.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn marks_load(ptr: *const u8, len: usize) -> u32 {
        said(Marks::load(unsafe { input(ptr, len) }).map(|marks| *held(&MARKS) = Some(marks)))
    }

    /// When the account's own held were worked out (epoch s); -1 where none are.
    #[unsafe(no_mangle)]
    pub extern "C" fn marks_at() -> f64 {
        held(&MARKS).as_ref().map_or(-1.0, |marks| marks.at() as f64)
    }

    /// What a device answers for itself made ready over the index, the sounds and the account's own held: 1 where there is no index
    /// to answer from. An account with nothing kept yet is one with nothing set.
    #[unsafe(no_mangle)]
    pub extern "C" fn device_open() -> u32 {
        let Some(index) = held(&INDEX).clone() else { return 1 };
        let marks = held(&MARKS).take().unwrap_or_default();
        *held(&DEVICE) = Some(Device::new(index, held(&SOUNDS).as_ref(), &marks));
        // They are the device's facts now: a second copy of the sounds is what the tablet could not hold.
        *held(&SOUNDS) = None;
        0
    }

    /// What is asked, as JSON (`Device::ask`): the answer as JSON, null where it has none.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn device_ask(ptr: *const u8, len: usize) -> *mut u8 {
        let Ok(asked) = std::str::from_utf8(unsafe { input(ptr, len) }) else { return core::ptr::null_mut() };
        held(&DEVICE).as_mut().and_then(|d| d.ask(asked)).map_or(core::ptr::null_mut(), |json| answer(json.as_bytes()))
    }

    /// `index::sync`'s state from the page's numbers: `tried` below zero for none.
    fn state(whole: u32, stopped: u32, lost: u32, failed: u32, tried: f64, started: f64) -> Part {
        Part { whole: whole != 0, stopped: stopped != 0, lost, failed, tried: (tried >= 0.0).then_some(tried as i64), started: started as i64 }
    }

    /// What to do now (`Part::step`). `network`: 0 free, 1 metered, 2 off; `mobile`: 0 unasked, 1 yes, 2 no; `told`: 1 when its feed may be asked. Its answer: a time to wait
    /// for (ms), or below zero: -1 stopped, -2 rest, -3 ask, -4 whole, -5 changes.
    #[unsafe(no_mangle)]
    #[allow(clippy::too_many_arguments)]
    pub extern "C" fn sync_step(whole: u32, stopped: u32, lost: u32, failed: u32, tried: f64, started: f64, network: u32, mobile: u32, told: u32, now: f64) -> f64 {
        let network = [Network::Free, Network::Metered][..].get(network as usize).copied().unwrap_or(Network::Off);
        let mobile = [Mobile::Unasked, Mobile::Yes][..].get(mobile as usize).copied().unwrap_or(Mobile::No);
        match state(whole, stopped, lost, failed, tried, started).step(network, mobile, told != 0, now as i64) {
            Step::Wait(until) => until as f64,
            Step::Stopped => -1.0,
            Step::Rest => -2.0,
            Step::Ask => -3.0,
            Step::Whole => -4.0,
            Step::Changes => -5.0,
        }
    }

    /// What to do now with an account's own (`Part::step_stale`): as `sync_step`, `took` when it was last taken, below zero for never.
    #[unsafe(no_mangle)]
    #[allow(clippy::too_many_arguments)]
    pub extern "C" fn sync_step_stale(whole: u32, stopped: u32, lost: u32, failed: u32, tried: f64, started: f64, network: u32, mobile: u32, took: f64, now: f64) -> f64 {
        let network = [Network::Free, Network::Metered][..].get(network as usize).copied().unwrap_or(Network::Off);
        let mobile = [Mobile::Unasked, Mobile::Yes][..].get(mobile as usize).copied().unwrap_or(Mobile::No);
        match state(whole, stopped, lost, failed, tried, started).step_stale(network, mobile, (took >= 0.0).then_some(took as i64), now as i64) {
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

    /// An ending as `sync_after` numbers it.
    fn number(outcome: Outcome) -> u32 {
        match outcome {
            Outcome::Took => 0,
            Outcome::Lost => 1,
            Outcome::Failed => 2,
            Outcome::StartAgain => 3,
            Outcome::Version => 4,
        }
    }

    /// How an answer ended a fetch (`sync::ended`), as `sync_after` numbers it, or 5 where it brought bytes to take. `status` below
    /// 0: no answer came.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_answer(status: i32, whole: u32) -> u32 {
        ended(u16::try_from(status).ok(), whole != 0).map_or(5, number)
    }

    /// The version of covers this module reads.
    #[unsafe(no_mangle)]
    pub extern "C" fn covers_version() -> u32 {
        COVERS_VERSION
    }

    /// Where the covers last taken stand; -1 where none were.
    #[unsafe(no_mangle)]
    pub extern "C" fn covers_rev_taken() -> f64 {
        *held(&COVERS)
    }

    /// Covers' head, taken if it reads.
    fn head_of(bytes: &[u8]) -> Result<(), Refused> {
        covers_rev(bytes).map(|rev| *held(&COVERS) = rev as f64)
    }

    /// Bytes just fetched whole, held if they can be: how that ended (`Outcome::of_whole`), as `sync_after` numbers it. `kind`: 0 the
    /// index, 1 the sounds, 2 the covers, by their head alone, 3 the account's own.
    ///
    /// # Safety
    /// `ptr` holds `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sync_take_whole(kind: u32, ptr: *const u8, len: usize) -> u32 {
        let bytes = unsafe { input(ptr, len) };
        number(match kind {
            0 => Outcome::of_whole(&Index::load(bytes).map(hold)),
            1 => Outcome::of_whole(&Sounds::load(bytes).map(|sounds| *held(&SOUNDS) = Some(sounds))),
            3 => Outcome::of_whole(&Marks::load(bytes).map(|marks| *held(&MARKS) = Some(marks))),
            _ => Outcome::of_whole(&head_of(bytes)),
        })
    }

    /// A feed applied to what is kept, the result held if it can be: how that ended (`Outcome::of_changes`), as `sync_after` numbers
    /// it. `kind` as `sync_take_whole`'s: for the covers `feed` is the feed's head, and nothing is kept.
    ///
    /// # Safety
    /// `kept` holds `kept_len` bytes, and `feed` `feed_len`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn sync_take_changes(kind: u32, kept: *const u8, kept_len: usize, feed: *const u8, feed_len: usize) -> u32 {
        let (kept, feed) = unsafe { (input(kept, kept_len), input(feed, feed_len)) };
        number(match kind {
            0 => {
                let applied = Index::load(kept).and_then(|mut index| Changes::load(feed).and_then(|c| index.apply(&c)).map(|()| index));
                Outcome::of_changes(&applied.map(hold))
            }
            1 => {
                let applied = Sounds::load(kept).and_then(|mut sounds| SoundChanges::load(feed).and_then(|c| sounds.apply(&c)).map(|()| sounds));
                Outcome::of_changes(&applied.map(|sounds| *held(&SOUNDS) = Some(sounds)))
            }
            _ => Outcome::of_changes(&head_of(feed)),
        })
    }

    /// How a part stands after a fetch (`Part::after`), or at a start (`Part::anew`, `Part::anew_by_another`). `outcome`: 0 took, 1 lost,
    /// 2 failed, 3 start again, 4 another version, 5 a start, 6 a start over what another version kept. Its answer in bits: 1 whole,
    /// 2 stopped, then lost from bit 8 and failed from bit 16.
    #[unsafe(no_mangle)]
    pub extern "C" fn sync_after(whole: u32, stopped: u32, lost: u32, failed: u32, outcome: u32) -> u32 {
        let ended = [Outcome::Took, Outcome::Lost, Outcome::Failed, Outcome::StartAgain, Outcome::Version][..].get(outcome as usize).copied();
        let s = state(whole, stopped, lost, failed, -1.0, 0.0);
        let s = match ended {
            Some(o) => s.after(o, 0),
            None if outcome == 5 => s.anew(0),
            None => s.anew_by_another(0),
        };
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

