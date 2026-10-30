//! The page's way in (the 0075 spike, the owner 2026-10-30): plain exports, no bindings to generate, no imports. Two
//! modules from this one crate (0.12): `rules.wasm`, the brake (feature `wasm-brake`), small and loaded with the page;
//! `matcher.wasm`, the matcher (feature `wasm-matcher`), fetched when the page is idle (scripts/rules-wasm.sh).

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
