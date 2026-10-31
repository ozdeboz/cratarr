//! Winding (decision 0074): the track from a moment, backwards or forwards, at a constant speed, as a tape deck winds;
//! anti-aliased as Tape rewind is (`rewind::read_block`, its half-band pyramid). The envelope is each player's own: its
//! vinyl ramp from still to 1 and back, never above 1×, on this stream. Open-ended (a hold: until the player stops
//! reading, or the edge), or condensed to a wall length with that envelope in mind (a tap, a winding seek): where the
//! speed would not get there in time the middle is cut into pieces spread evenly over the way, each jump crossfaded;
//! where it would get there sooner the speed is lowered. Quieter than the music by the device's wind volume. The map
//! (`map`) says where each moment of the stream stands in the track.

use std::f64::consts::PI;

use anyhow::{Result, bail};

use crate::rewind::{BLOCK, FADE, FIRST, SPAN, Sound, XFADE, read_block};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Back,
    Forward,
}

/// A condensed wind: its wall length and the player's envelope on it (milliseconds), and the pieces' length. `fly`:
/// the player's rate rests on a floor where the wind sounds at the track's own speed (1 / its speed), so it takes over
/// from the track and hands back to it moving (0.9.14, the owner: a seek forward need not stop first).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub ms: u64,
    pub attack_ms: u64,
    pub release_ms: u64,
    pub piece_ms: u64,
    pub fly: bool,
}

/// What a player asks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ask {
    pub dir: Dir,
    pub from_ms: u64,
    /// Where it ends: the track's edge when none (0:00 back, its end forward).
    pub to_ms: Option<u64>,
    /// The sustain level (× the track's speed).
    pub speed: f64,
    pub gain_db: f64,
    pub fit: Option<Fit>,
    /// A hold that speeds up as it goes (0.9.12, the owner: "doubling every 1.5 s, up to 64×"): open-ended only.
    pub accel: Option<Accel>,
}

/// A hold's speed at stream time t (seconds): min(max, speed × 2^(t / period)); its way so far, the integral of that.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accel {
    pub period_s: f64,
    pub max: f64,
    /// Flying past what is heard (0.9.14, `flight_max`; the owner: pure): heard at most at this speed, and past it in
    /// pieces of this many ms each from where the flight has got, the way between jumped over; `max` is then the
    /// flight's top. None: heard as it goes, up to `max`.
    pub audio: Option<(f64, u64)>,
    /// A let-go's brake instead of a rise (0.9.14-3): the stream carries the plan's own curve, played at 1×.
    pub brake: Option<Brake>,
}

/// A let-go's one plan (the app's and the page's, 2026-10-28; the owner: let the stream carry the motion): from `v0` ×
/// to `to` × over `secs`, a raised cosine, its stream beginning `at` seconds into it; past its end, `to` ×. A device
/// plays it at 1×, so its speed is the stream's, whatever the device's own output reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brake {
    pub v0: f64,
    pub to: f64,
    pub secs: f64,
    pub at: f64,
}

impl Brake {
    /// Its speed `s` seconds into it (×): the players' one brake (`cratarr_rules::brake`).
    fn v(&self, s: f64) -> f64 {
        cratarr_rules::brake::v(self.v0, self.to, self.secs * 1000.0, s * 1000.0)
    }

    /// Its way from its start to `s` seconds into it (seconds of track).
    fn w(&self, s: f64) -> f64 {
        cratarr_rules::brake::way(self.v0, self.to, self.secs * 1000.0, s * 1000.0) / 1000.0
    }

    /// Where its stream ends (stream seconds): at the brake's end, the aim.
    pub fn end(&self) -> f64 {
        (self.secs - self.at).max(0.0)
    }

    /// When (stream seconds) it slows below `top` ×; none where it never is above it.
    fn below(&self, top: f64) -> Option<f64> {
        if self.v(self.at) <= top {
            return None;
        }
        let k = ((top - self.to) / (self.v0 - self.to)).clamp(0.0, 1.0);
        Some(self.secs / PI * (2.0 * k - 1.0).acos() - self.at)
    }
}

impl Accel {
    /// When it reaches its top speed (stream seconds) from `base`.
    fn top_at(&self, base: f64) -> f64 {
        (self.period_s * (self.max.max(base) / base).log2()).max(0.0)
    }

    /// How far it has gone by stream time `t` (seconds of track), from `base`.
    pub fn gone(&self, base: f64, t: f64) -> f64 {
        if let Some(b) = self.brake {
            return b.w(b.at + t) - b.w(b.at);
        }
        let p = self.period_s;
        let ts = self.top_at(base);
        let up = |t: f64| base * p / std::f64::consts::LN_2 * (2f64.powf(t / p) - 1.0);
        if t <= ts { up(t) } else { up(ts) + self.max.max(base) * (t - ts) }
    }

    /// When it has gone `d` (seconds of track), from `base`.
    pub fn when(&self, base: f64, d: f64) -> f64 {
        if let Some(b) = self.brake {
            // Its way only grows, so halved down to it; one braking to still goes no further than its end.
            let end = b.end();
            if d >= self.gone(base, end) {
                return if b.to > 0.0 { end + (d - self.gone(base, end)) / b.to } else { end };
            }
            let (mut lo, mut hi) = (0.0, end);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if self.gone(base, mid) < d { lo = mid } else { hi = mid }
            }
            return (lo + hi) / 2.0;
        }
        let p = self.period_s;
        let ts = self.top_at(base);
        let at_top = self.gone(base, ts);
        if d <= at_top { p * (1.0 + d * std::f64::consts::LN_2 / (base * p)).log2() } else { ts + (d - at_top) / self.max.max(base) }
    }

    /// Its speed at stream time `t` (×).
    fn speed(&self, base: f64, t: f64) -> f64 {
        if let Some(b) = self.brake {
            return b.v(b.at + t);
        }
        (base * 2f64.powf(t / self.period_s)).min(self.max.max(base))
    }

    /// Where it is above its audio top, pieces (stream seconds): a rise from where it passes it on; a brake until it
    /// slows below it. None where it never is.
    fn above(&self, base: f64, top: f64) -> Option<(f64, f64)> {
        match self.brake {
            Some(b) => b.below(top).map(|e| (-b.at, e)),
            None => (self.max > top).then(|| ((self.period_s * (top / base).log2()).max(0.0), f64::INFINITY)),
        }
    }
}

/// A hold's pieces (`frames` of its stream, from stream time `t0`, the track at `x`): one as it goes; flying past its
/// audio top, continuous up to it, then a piece every `piece` ms of the stream on one grid from that moment (on
/// through the queue's joins), each from where the flight has got (`Accel::gone`), heard at the audio top, crossfaded
/// at each cut as a condensed wind's.
#[allow(clippy::too_many_arguments)]
fn flight_pieces(a: &Accel, base: f64, t0: f64, x: f64, sign: f64, rate: f64, out: f64, frames: usize) -> Vec<Piece> {
    let whole = vec![Piece { from: 0, to: frames, top: x, fade_in: None, fade_out: None, own: (0, frames) }];
    let Some((top, piece_ms)) = a.audio else { return whole };
    // Where it is above its audio top (stream time), and so where the grid of cuts runs, in this track's frames; past
    // a brake's slowing below it, one piece heard as it goes.
    let Some((cross, until)) = a.above(base, top) else { return whole };
    let grid = (cross - t0) * out;
    let last = ((until - t0) * out).min(frames as f64);
    let step = (piece_ms as f64 / 1000.0 * out).max(1.0);
    let mut starts = vec![0usize];
    let mut k = if grid > 0.0 { 0.0 } else { (-grid / step).ceil() };
    loop {
        let s = (grid + k * step).round();
        if s >= last {
            break;
        }
        if s > 0.0 {
            starts.push(s as usize);
        }
        k += 1.0;
    }
    if last > 0.0 && last < frames as f64 && starts.last().is_some_and(|s| (*s as f64) < last.round()) {
        starts.push(last.round() as usize);
    }
    if starts.len() == 1 {
        return whole;
    }
    let half = ((XFADE * out / 2.0) as usize).max(1);
    let heard = top * rate / out;
    let place = |s: usize| x + sign * (a.gone(base, t0 + s as f64 / out) - a.gone(base, t0)) * rate;
    let n = starts.len();
    (0..n)
        .map(|j| {
            let (s, e) = (starts[j], if j + 1 < n { starts[j + 1] } else { frames });
            let from = if j == 0 { s } else { s.saturating_sub(half) };
            Piece {
                from,
                to: if j + 1 == n { e } else { (e + half).min(frames) },
                // Before its own first frame it is where the heard tape would have been, at the audio top.
                top: place(s) - sign * heard * (s - from) as f64,
                fade_in: (j > 0).then_some((from, s + half)),
                fade_out: (j + 1 < n).then_some((e.saturating_sub(half), (e + half).min(frames))),
                own: (s, e),
            }
        })
        .collect()
}

/// Below this speed (×) a wind fades with it (`Wind::still`).
const STILL_BELOW: f64 = 0.25;

/// A condensed wind is at most this long (a tap's and a seek's cap, the owner).
const LONGEST_FIT_MS: u64 = 12_000;
const SHORTEST_FIT_MS: u64 = 200;

/// One stretch of the stream: its frames with the crossfades' overlap, where the track is at its first, its fades; and
/// its own frames without the overlap, for the map.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    from: usize,
    to: usize,
    top: f64,
    fade_in: Option<(usize, usize)>,
    fade_out: Option<(usize, usize)>,
    own: (usize, usize),
}

impl Piece {
    fn gain(&self, i: usize) -> f64 {
        let ease = |(a, b): (usize, usize), up: bool| {
            let t = ((i as f64 + 0.5 - a as f64) / (b - a) as f64).clamp(0.0, 1.0);
            if up { (t * PI / 2.0).sin() } else { (t * PI / 2.0).cos() }
        };
        self.fade_in.map_or(1.0, |w| if i < w.1 { ease(w, true) } else { 1.0 }) * self.fade_out.map_or(1.0, |w| if i >= w.0 { ease(w, false) } else { 1.0 })
    }
}

/// The player's envelope at wall frame `i` of `d`: raised-cosine up over `a`, down over `r`; a wind too short for both
/// peaks lower (the release starts from wherever the attack had got to).
fn envelope(i: usize, d: usize, a: usize, r: usize) -> f64 {
    let ramp = |x: f64, len: usize| if len == 0 { 1.0 } else { (1.0 - (PI * (x / len as f64).clamp(0.0, 1.0)).cos()) / 2.0 };
    let t = i as f64 + 0.5;
    ramp(t, a).min(ramp(d as f64 - t, r))
}

pub struct Wind {
    rate: u32,
    out_rate: u32,
    channels: usize,
    /// −1 back, +1 forward.
    sign: f64,
    /// The stream's length (frames of the stream, the clip time a player consumes).
    frames: usize,
    /// Frames of the track a frame of the stream.
    v: f64,
    pieces: Vec<Piece>,
    gain: f64,
    /// An accelerating hold: its rise, the speed it starts from (×), the stream time gone before it (seconds), and
    /// where it begins and ends (ms), which its map says.
    accel: Option<Accel>,
    base: f64,
    t0: f64,
    ends: Option<(u64, u64)>,
}

impl Wind {
    pub fn new(rate: u32, channels: usize, duration_ms: Option<u64>, ask: Ask) -> Result<Self> {
        Self::joined(rate, channels, duration_ms, ask, None)
    }

    /// As `new`, its rate and channels those of the stream it joins (`Some`): winding on through the queue, every track
    /// in the one stream, whatever its own.
    pub fn joined(rate: u32, channels: usize, duration_ms: Option<u64>, ask: Ask, stream: Option<(u32, usize)>) -> Result<Self> {
        Self::joined_at(rate, channels, duration_ms, ask, stream, 0.0)
    }

    /// As `joined`, an accelerating hold's stream time `t0` (seconds) already gone before it (the tracks before it in
    /// the chain): its speed carries on from there.
    fn joined_at(rate: u32, channels: usize, duration_ms: Option<u64>, ask: Ask, stream: Option<(u32, usize)>, t0: f64) -> Result<Self> {
        let out_rate = if let Some((r, _)) = stream {
            r
        } else if rate <= 48_000 {
            rate
        } else if rate % 44_100 == 0 {
            44_100
        } else {
            48_000
        };
        let channels = stream.map_or(channels, |(_, c)| c);
        let speed = if ask.speed.is_finite() { ask.speed.clamp(1.0, 16.0) } else { 6.0 };
        let edge = match ask.dir {
            Dir::Back => 0,
            Dir::Forward => match duration_ms {
                Some(d) => d,
                None => bail!("the track's length is not known: it cannot be wound forward"),
            },
        };
        let from_ms = duration_ms.map_or(ask.from_ms, |d| ask.from_ms.min(d));
        let to_ms = ask.to_ms.unwrap_or(edge);
        let to_ms = duration_ms.map_or(to_ms, |d| to_ms.min(d));
        let sign = if ask.dir == Dir::Back { -1.0 } else { 1.0 };
        if (to_ms as f64 - from_ms as f64) * sign <= 0.0 {
            bail!("nothing to wind: the end is not that way");
        }
        let at = |ms: u64| ms as f64 * f64::from(rate) / 1000.0;
        let (x, t) = (at(from_ms), at(to_ms));
        let distance = (t - x).abs();
        let mut v = speed * f64::from(rate) / f64::from(out_rate);
        let per_ms = f64::from(out_rate) / 1000.0;
        let accel = ask.accel.filter(|_| ask.fit.is_none());
        let mut w = Wind { rate, out_rate, channels: channels.clamp(1, 2), sign, frames: 0, v, pieces: Vec::new(), gain: 10f64.powf(ask.gain_db.clamp(-48.0, 0.0) / 20.0), accel, base: speed, t0, ends: None };
        if let Some(a) = accel {
            // As long as it takes at its rising speed: from stream time t0 to where the way is gone.
            let secs = distance / f64::from(rate);
            let mut end = a.when(speed, a.gone(speed, t0) + secs);
            let mut to_ms = to_ms;
            // A brake ends at its aim, in this track or a later one.
            if let Some(b) = a.brake
                && end > b.end()
            {
                end = b.end();
                to_ms = (from_ms as f64 + sign * (a.gone(speed, end) - a.gone(speed, t0)) * 1000.0).round().max(0.0) as u64;
            }
            w.frames = ((end - t0) * f64::from(out_rate)).ceil().max(1.0) as usize;
            w.pieces = flight_pieces(&a, speed, t0, x, sign, f64::from(rate), f64::from(out_rate), w.frames);
            w.ends = Some((from_ms, to_ms));
            return Ok(w);
        }
        let Some(fit) = ask.fit else {
            // Open-ended: as long as the way at the speed asked.
            w.frames = (distance / v).ceil().max(1.0) as usize;
            w.pieces = vec![Piece { from: 0, to: w.frames, top: x, fade_in: None, fade_out: None, own: (0, w.frames) }];
            return Ok(w);
        };
        let d = (fit.ms.clamp(SHORTEST_FIT_MS, LONGEST_FIT_MS) as f64 * per_ms) as usize;
        let (a, r) = ((fit.attack_ms as f64 * per_ms) as usize, (fit.release_ms as f64 * per_ms) as usize);
        // The stream the player consumes over its wall length, its envelope's area.
        let area: f64 = (0..d).map(|i| envelope(i, d, a, r)).sum();
        // Flying: the envelope on its floor f (the track's own speed, `k` track frames a stream frame, over the wind's
        // `v`): f·D + (1 − f)·area. Only where it winds faster than the track plays and further than it would play.
        let k = f64::from(rate) / f64::from(out_rate);
        let fly = fit.fly && v > k && distance > k * d as f64 && area > 0.0;
        let clip = if fly { k / v * d as f64 + (1.0 - k / v) * area } else { area };
        w.frames = clip.round().max(1.0) as usize;
        if v * w.frames as f64 >= distance {
            // Not cut: slower than asked, just fast enough. Flying, the speed whose stream on its floor is the way:
            // distance = k·D + (v − k)·area.
            v = if fly { k + (distance - k * d as f64) / area } else { distance / w.frames as f64 };
            w.v = v;
            if fly {
                w.frames = (distance / v).round().max(1.0) as usize;
            }
            let n = w.frames;
            w.pieces = vec![Piece { from: 0, to: n, top: x, fade_in: None, fade_out: None, own: (0, n) }];
            return Ok(w);
        }
        let n = w.frames;
        // Cut: pieces of the clip spread evenly over the way, the first from X, the last ending on T.
        let piece = (fit.piece_ms as f64 * per_ms).max(1.0);
        let k = ((n as f64 / piece).round() as usize).max(2);
        let edge = |i: usize| (i * n + k / 2) / k;
        let last_top = t - sign * v * (edge(k) - edge(k - 1)) as f64;
        let half = ((XFADE * f64::from(out_rate) / 2.0) as usize).max(1);
        w.pieces = (0..k)
            .map(|i| {
                let (s, e) = (edge(i), edge(i + 1));
                let top = x + (last_top - x) * i as f64 / (k - 1) as f64;
                let from = if i == 0 { s } else { s.saturating_sub(half) };
                Piece {
                    from,
                    to: if i == k - 1 { e } else { (e + half).min(n) },
                    // Before its own first frame it is where the track was a little earlier in the wind.
                    top: top - sign * v * (s - from) as f64,
                    fade_in: (i > 0).then_some((from, s + half)),
                    fade_out: (i < k - 1).then_some((e.saturating_sub(half), (e + half).min(n))),
                    own: (s, e),
                }
            })
            .collect();
        Ok(w)
    }

    /// Frames of the track a frame of the stream, at stream frame `i`: steady, or an accelerating hold's speed then.
    fn step(&self, i: usize) -> f64 {
        match self.accel {
            None => self.v,
            // Flying past its audio top: heard at that top, its pieces placed by the flight.
            Some(a) => {
                let v = a.speed(self.base, self.t0 + (i as f64 + 0.5) / f64::from(self.out_rate));
                a.audio.map_or(v, |(top, _)| v.min(top)) * f64::from(self.rate) / f64::from(self.out_rate)
            }
        }
    }

    /// The whole WAV's length (bytes): one track alone (the endpoint sums a chain's parts).
    #[cfg(test)]
    pub fn bytes(&self) -> u64 {
        44 + self.data()
    }

    /// Its sound's length (bytes), without the header.
    pub fn data(&self) -> u64 {
        self.frames as u64 * self.channels as u64 * 2
    }

    /// The same wind on a track whose own rate is `rate` (0.9.14-2: a chain is planned at the stream's rate, each track
    /// read only as the wind reaches it): its length, pieces and map in the stream as they were, its places in the track
    /// at that rate. A hold's length is the way's seconds over its speed, whatever the track's rate.
    pub fn at_rate(&self, rate: u32) -> Wind {
        let k = f64::from(rate) / f64::from(self.rate);
        Wind {
            rate,
            out_rate: self.out_rate,
            channels: self.channels,
            sign: self.sign,
            frames: self.frames,
            v: self.v * k,
            pieces: self.pieces.iter().map(|p| Piece { top: p.top * k, ..p.clone() }).collect(),
            gain: self.gain,
            accel: self.accel,
            base: self.base,
            t0: self.t0,
            ends: self.ends,
        }
    }

    /// Below a quarter of the track's own speed it fades with its speed (a brake to still: what a tape does as it
    /// stops, no click at the end); `v` track frames a stream frame.
    fn still(&self, v: f64) -> f64 {
        (v * f64::from(self.out_rate) / f64::from(self.rate) / STILL_BELOW).min(1.0)
    }

    /// Its length (ms of the stream).
    pub fn ms(&self) -> f64 {
        self.frames as f64 * 1000.0 / f64::from(self.out_rate)
    }

    /// The stream's rate and channels, for the tracks that join it.
    pub fn stream(&self) -> (u32, usize) {
        (self.out_rate, self.channels)
    }

    /// The speed it winds at (× the track's): lower than asked where a condensed wind got there sooner.
    pub fn speed(&self) -> f64 {
        self.v * f64::from(self.out_rate) / f64::from(self.rate)
    }

    /// Where each piece of the stream stands in the track: knots (stream ms, track ms), linear between; a cut is two
    /// knots at one stream moment.
    pub fn map(&self) -> Vec<(u64, u64)> {
        // An accelerating hold: its ends (the way between, its formula: `Accel::gone`).
        if let Some((from, to)) = self.ends {
            return vec![(0, from), ((self.frames as f64 * 1000.0 / f64::from(self.out_rate)).round() as u64, to)];
        }
        let ms = |frames: f64, rate: u32| (frames * 1000.0 / f64::from(rate)).round().max(0.0) as u64;
        self.pieces
            .iter()
            .flat_map(|p| {
                let top = p.top + self.sign * self.v * (p.own.0 - p.from) as f64;
                let end = top + self.sign * self.v * (p.own.1 - p.own.0) as f64;
                [(ms(p.own.0 as f64, self.out_rate), ms(top, self.rate)), (ms(p.own.1 as f64, self.out_rate), ms(end, self.rate))]
            })
            .collect()
    }

    /// The WAV, header first, a block at a time to `send`; one that answers false (the player stopped reading: a hold
    /// let go) ends it. Only what the crossfades still need is kept, however long the wind.
    #[cfg(test)]
    pub fn render(&self, sound: &mut dyn Sound, mut send: impl FnMut(Vec<u8>) -> bool) -> Result<()> {
        if !send(crate::rewind::wav_header(self.out_rate, self.channels, self.data())) {
            return Ok(());
        }
        self.sound(sound, &mut Shaper::new(self.channels), send).map(|_| ())
    }

    /// Its sound without the header (a track joining a stream); whether the player is still reading.
    pub fn sound(&self, sound: &mut dyn Sound, shaper: &mut Shaper, mut send: impl FnMut(Vec<u8>) -> bool) -> Result<bool> {
        let fade = ((FADE * f64::from(self.out_rate)) as usize).max(1);
        let span = SPAN * f64::from(self.rate);
        let mut dither = crate::convert::Tpdf::new();
        // The frames not sent yet, from `sent` on.
        let mut pending: Vec<Vec<f32>> = vec![Vec::new(); self.channels];
        let mut sent = 0;
        for (j, p) in self.pieces.iter().enumerate() {
            let (mut i, mut at) = (p.from, p.top);
            while i < p.to {
                let most = (((if i == 0 { FIRST } else { BLOCK }) * f64::from(self.out_rate)) as usize).max(1);
                let (start, here) = (i, at);
                let (mut ats, mut speeds) = (Vec::with_capacity(most), Vec::with_capacity(most));
                while i < p.to && i - start < most && (at - here).abs() < span {
                    ats.push(at.max(0.0));
                    let v = self.step(i);
                    speeds.push(v);
                    at += self.sign * v;
                    i += 1;
                }
                let block = read_block(sound, self.channels, &ats, &speeds)?;
                for (c, b) in pending.iter_mut().zip(&block) {
                    if c.len() < i - sent {
                        c.resize(i - sent, 0.0);
                    }
                    for (k, v) in b.iter().enumerate() {
                        c[start + k - sent] += (f64::from(*v) * p.gain(start + k) * self.gain * self.still(speeds[k])) as f32;
                    }
                }
                // What no later piece adds to is ready.
                let ready = self.pieces.get(j + 1).map_or(i, |next| i.min(next.from));
                if ready > sent {
                    // A rising hold keeps its sound as it speeds up (`Shaper`).
                    let mut chunk: Vec<Vec<f64>> = pending.iter().map(|c| c[..ready - sent].iter().map(|v| f64::from(*v)).collect()).collect();
                    if self.accel.is_some() {
                        let speed = self.step((sent + ready) / 2) * f64::from(self.out_rate) / f64::from(self.rate);
                        shaper.shape(&mut chunk, speed, f64::from(self.out_rate));
                    }
                    let mut bytes = Vec::with_capacity((ready - sent) * self.channels * 2);
                    for f in sent..ready {
                        let edge = f.min(self.frames - 1 - f);
                        let g = if edge < fade { (1.0 - (PI * (edge as f64 + 0.5) / fade as f64).cos()) / 2.0 } else { 1.0 };
                        for c in &chunk {
                            let v = c[f - sent] * g * 32767.0 + dither.next();
                            bytes.extend_from_slice(&(v.round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
                        }
                    }
                    for c in pending.iter_mut() {
                        c.drain(..ready - sent);
                    }
                    sent = ready;
                    if !send(bytes) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
}

/// A rising hold's sound kept as it speeds up (0.9.12, the owner: "I don't want the sound effect to be drastically
/// different"): faster, a wind's music rises in pitch, brighter and harsher, and its loudness drifts. Above 8× a
/// low-pass whose corner falls as the speed rises (20 kHz × 8 / speed, never below 2.5 kHz) keeps it the same whoosh;
/// a leveller holds it as loud as its first half second (±6 dB at most, eased over each block, so it never pumps).
/// One for the whole stream, on through the queue's joins.
pub struct Shaper {
    /// Each channel's filter memory (x₁, x₂, y₁, y₂).
    memory: Vec<[f64; 4]>,
    /// The first half second's loudness (its mean square while it is heard), once known.
    reference: Option<f64>,
    heard: (f64, usize),
    gain: f64,
}

/// How long the loudness it keeps is heard for (stream seconds), and how far it may move it.
const SHAPER_REFERENCE_S: f64 = 0.5;
const SHAPER_MOST: f64 = 2.0;

impl Shaper {
    pub fn new(channels: usize) -> Self {
        Self { memory: vec![[0.0; 4]; channels], reference: None, heard: (0.0, 0), gain: 1.0 }
    }

    /// Shapes one chunk (planar) heard at `speed` ×, the stream at `rate`.
    fn shape(&mut self, chunk: &mut [Vec<f64>], speed: f64, rate: f64) {
        let n = chunk.first().map_or(0, Vec::len);
        if n == 0 {
            return;
        }
        // The low-pass (RBJ's, Q 0.707), its corner for this chunk's speed.
        if speed > 8.0 {
            let fc = (20_000.0 * 8.0 / speed).max(2_500.0).min(0.45 * rate);
            let w = 2.0 * PI * fc / rate;
            let (cw, alpha) = (w.cos(), w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2));
            let a0 = 1.0 + alpha;
            let (b0, b1, a1, a2) = ((1.0 - cw) / 2.0 / a0, (1.0 - cw) / a0, -2.0 * cw / a0, (1.0 - alpha) / a0);
            for (c, m) in chunk.iter_mut().zip(self.memory.iter_mut()) {
                for x in c.iter_mut() {
                    let y = b0 * *x + b1 * m[0] + b0 * m[1] - a1 * m[2] - a2 * m[3];
                    (m[1], m[0], m[3], m[2]) = (m[0], *x, m[2], y);
                    *x = y;
                }
            }
        }
        // The leveller: as loud as the first half second.
        let square = chunk.iter().flat_map(|c| c.iter()).map(|x| x * x).sum::<f64>() / (n * chunk.len()) as f64;
        let reference = match self.reference {
            Some(r) => r,
            None => {
                self.heard = (self.heard.0 + square * n as f64, self.heard.1 + n);
                if (self.heard.1 as f64) < SHAPER_REFERENCE_S * rate {
                    return;
                }
                let r = self.heard.0 / self.heard.1 as f64;
                self.reference = Some(r);
                r
            }
        };
        let want = if square > 1e-10 && reference > 1e-10 { (reference / square).sqrt().clamp(1.0 / SHAPER_MOST, SHAPER_MOST) } else { self.gain };
        let (from, to) = (self.gain, self.gain + (want - self.gain) * 0.5);
        for c in chunk.iter_mut() {
            for (i, x) in c.iter_mut().enumerate() {
                *x *= from + (to - from) * (i as f64 + 0.5) / n as f64;
            }
        }
        self.gain = to;
    }
}

/// Winding on through the queue (0074, the queue as one tape): this track's wind to its edge, then each track in the
/// way, nearest first, wound whole (back: from its end; forward: from its start), at the same speed and gain, in one
/// stream. Each join stands exactly at a track's length as the catalogue reads it. `tracks`: each one's rate, channels
/// and length (ms); the chain ends before the first whose length is not known.
pub fn chain(tracks: &[(u32, usize, Option<u64>)], ask: Ask) -> Result<Vec<Wind>> {
    let Some(&(rate, channels, duration)) = tracks.first() else { bail!("nothing to wind") };
    let first = Wind::new(rate, channels, duration, Ask { fit: None, to_ms: None, ..ask })?;
    let stream = first.stream();
    let mut t0 = first.ms() / 1000.0;
    let mut out = vec![first];
    for &(rate, channels, duration) in &tracks[1..] {
        // A brake's stream ends at its aim: no track past it.
        if ask.accel.and_then(|a| a.brake).is_some_and(|b| t0 >= b.end() - 1e-9) {
            break;
        }
        let Some(d) = duration.filter(|d| *d > 0) else { break };
        let from_ms = if ask.dir == Dir::Back { d } else { 0 };
        let w = Wind::joined_at(rate, channels, Some(d), Ask { from_ms, to_ms: None, fit: None, ..ask }, Some(stream), t0)?;
        t0 += w.ms() / 1000.0;
        out.push(w);
    }
    Ok(out)
}

/// A chain's map: knots (stream ms, which track, track ms), each track's own after the ones before it.
pub fn chain_map(winds: &[Wind]) -> Vec<(u64, usize, u64)> {
    let mut at = 0.0;
    let mut out = Vec::new();
    for (i, w) in winds.iter().enumerate() {
        out.extend(w.map().into_iter().map(|(c, t)| ((at + c as f64).round() as u64, i, t)));
        at += w.ms();
    }
    out
}

/// A wind's volume when a player asks none (dB): the tape's start (shared/rules.json `tape.gain_db.start`; the owner,
/// 2026-10-31: -6, was -12).
pub const GAIN_DB_START: f64 = -6.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The player rules' one table (shared/rules.json; the audit, 2026-10-29), which the app's and the page's tests
    /// check too.
    fn rules() -> serde_json::Value {
        serde_json::from_str(include_str!("../../shared/rules.json")).unwrap()
    }

    #[test]
    fn the_winds_rules_and_a_brake_are_the_tables() {
        let r = rules();
        assert_eq!(r["wind_cap_ms"].as_u64(), Some(LONGEST_FIT_MS));
        assert_eq!(r["wind_shortest_ms"].as_u64(), Some(SHORTEST_FIT_MS));
        assert_eq!(r["tape"]["gain_db"]["start"].as_f64(), Some(GAIN_DB_START));
        for (i, b) in r["brake"].as_array().unwrap().iter().enumerate() {
            let brake = Brake { v0: b["v0"].as_f64().unwrap(), to: b["to"].as_f64().unwrap(), secs: b["length_ms"].as_f64().unwrap() / 1000.0, at: 0.0 };
            for p in b["at"].as_array().unwrap() {
                let s = p["t_ms"].as_f64().unwrap() / 1000.0;
                assert!((brake.v(s) - p["v"].as_f64().unwrap()).abs() < 1e-9, "brake {i} at {s} s: speed");
                assert!((brake.w(s) * 1000.0 - p["way_ms"].as_f64().unwrap()).abs() < 1e-6, "brake {i} at {s} s: way");
            }
        }
    }

    struct Tone {
        hz: f64,
        secs: f64,
    }

    impl Sound for Tone {
        fn read(&mut self, from: u64, to: u64) -> Result<(u64, Vec<Vec<f32>>)> {
            let to = to.min((self.secs * 44_100.0) as u64);
            let one: Vec<f32> = (from..to).map(|i| (0.5 * (2.0 * PI * self.hz * i as f64 / 44_100.0).sin()) as f32).collect();
            Ok((from, vec![one.clone(), one]))
        }
    }

    fn ask(dir: Dir, from_ms: u64, speed: f64) -> Ask {
        Ask { dir, from_ms, to_ms: None, speed, gain_db: 0.0, fit: None, accel: None }
    }

    fn wav(w: &Wind, sound: &mut dyn Sound) -> Vec<u8> {
        let mut out = Vec::new();
        w.render(sound, |b| {
            out.extend_from_slice(&b);
            true
        })
        .unwrap();
        out
    }

    #[test]
    fn a_hold_winds_to_the_edge_at_its_speed_either_way() {
        // Ten seconds back at 2 ×: five seconds of stream, from 10 s to 0.
        let w = Wind::new(44_100, 2, Some(60_000), ask(Dir::Back, 10_000, 2.0)).unwrap();
        assert_eq!(w.frames, 5 * 44_100);
        assert_eq!(w.map(), vec![(0, 10_000), (5_000, 0)]);
        assert_eq!(wav(&w, &mut Tone { hz: 440.0, secs: 60.0 }).len() as u64, w.bytes());
        // Forward from 10 s to a seek's target at 40 s, at 4 ×: 7.5 s.
        let w = Wind::new(44_100, 2, Some(60_000), Ask { to_ms: Some(40_000), ..ask(Dir::Forward, 10_000, 4.0) }).unwrap();
        assert_eq!(w.map(), vec![(0, 10_000), (7_500, 40_000)]);
        // Nothing that way.
        assert!(Wind::new(44_100, 2, Some(60_000), Ask { to_ms: Some(5_000), ..ask(Dir::Forward, 10_000, 4.0) }).is_err());
    }

    #[test]
    fn a_tap_fits_its_length_with_the_players_envelope() {
        let fit = Fit { ms: 2_000, attack_ms: 300, release_ms: 200, piece_ms: 350, fly: false };
        // Three minutes back: the stream is the envelope's area (2 s less half of 0.5 s), cut into pieces ending on 0:00.
        let w = Wind::new(44_100, 2, Some(240_000), Ask { fit: Some(fit), ..ask(Dir::Back, 180_000, 6.0) }).unwrap();
        assert!((w.frames as f64 / 44_100.0 - 1.75).abs() < 0.01, "{}", w.frames);
        assert_eq!(w.pieces.len(), 5);
        let map = w.map();
        assert_eq!((map[0].1, map.last().unwrap().1), (180_000, 0));
        assert!((w.speed() - 6.0).abs() < 1e-9);
        // Five seconds back: not cut, slower than asked.
        let w = Wind::new(44_100, 2, Some(240_000), Ask { fit: Some(fit), ..ask(Dir::Back, 5_000, 6.0) }).unwrap();
        assert_eq!(w.pieces.len(), 1);
        assert!((w.speed() - 5.0 / 1.75).abs() < 0.01, "{}", w.speed());
        // Shorter than its attack and release: it peaks lower, so it consumes less.
        let short = Fit { ms: 400, ..fit };
        let w = Wind::new(44_100, 2, Some(240_000), Ask { fit: Some(short), ..ask(Dir::Back, 180_000, 6.0) }).unwrap();
        assert!(w.frames < 44_100 / 5, "{}", w.frames);
    }

    #[test]
    fn a_flying_tap_rests_on_the_tracks_own_speed() {
        let fit = Fit { ms: 2_000, attack_ms: 300, release_ms: 200, piece_ms: 350, fly: true };
        // A minute on at 16×: cut, the stream the envelope on its floor of 1/16 (2 s / 16 + 15/16 of 1.75 s).
        let w = Wind::new(44_100, 2, Some(240_000), Ask { to_ms: Some(70_000), fit: Some(fit), ..ask(Dir::Forward, 10_000, 16.0) }).unwrap();
        assert!((w.frames as f64 / 44_100.0 - (2.0 / 16.0 + 15.0 / 16.0 * 1.75)).abs() < 0.01, "{}", w.frames);
        assert!(w.pieces.len() > 1);
        assert!((w.speed() - 16.0).abs() < 1e-9);
        // 20 s on: not cut; the speed whose stream on its floor (1/v) covers the way: 1 + (20 − 2) / 1.75.
        let w = Wind::new(44_100, 2, Some(240_000), Ask { to_ms: Some(30_000), fit: Some(fit), ..ask(Dir::Forward, 10_000, 16.0) }).unwrap();
        assert_eq!(w.pieces.len(), 1);
        let v = 1.0 + 18.0 / 1.75;
        assert!((w.speed() - v).abs() < 0.01, "{}", w.speed());
        let floor = 1.0 / w.speed();
        let consumed = 2.0 * floor + (1.0 - floor) * 1.75;
        assert!((w.frames as f64 / 44_100.0 - consumed).abs() < 0.01, "{} {consumed}", w.frames);
        let map = w.map();
        assert_eq!(map[0], (0, 10_000));
        assert!(map.last().unwrap().1.abs_diff(30_000) <= 2, "{map:?}");
        // Without `fly` nothing changes: as before.
        let plain = Wind::new(44_100, 2, Some(240_000), Ask { to_ms: Some(30_000), fit: Some(Fit { fly: false, ..fit }), ..ask(Dir::Forward, 10_000, 16.0) }).unwrap();
        assert!((plain.speed() - 20.0 / 1.75).abs() < 0.01, "{}", plain.speed());
    }

    #[test]
    fn a_hold_winds_on_through_the_queue_joining_at_each_tracks_length() {
        // Back from 10 s at 2 ×: 5 s to this track's start, then the one before (30 s) whole from its end, 15 s more; the
        // one before that has no known length and ends the chain.
        let winds = chain(&[(44_100, 2, Some(60_000)), (48_000, 1, Some(30_000)), (44_100, 2, None)], ask(Dir::Back, 10_000, 2.0)).unwrap();
        assert_eq!(winds.len(), 2);
        assert_eq!(winds[1].stream(), (44_100, 2), "one stream, whatever the track's own");
        assert_eq!(chain_map(&winds), vec![(0, 0, 10_000), (5_000, 0, 0), (5_000, 1, 30_000), (20_000, 1, 0)]);
        // The sound joins without a gap: the whole stream as long as its parts.
        let mut wav = Vec::new();
        let (a, b) = (&winds[0], &winds[1]);
        assert!(a.render(&mut Tone { hz: 440.0, secs: 60.0 }, |x| { wav.extend_from_slice(&x); true }).is_ok());
        assert!(b.sound(&mut Tone { hz: 440.0, secs: 30.0 }, &mut Shaper::new(2), |x| { wav.extend_from_slice(&x); true }).unwrap());
        assert_eq!(wav.len() as u64, a.bytes() + b.data());
        // Forward: each from its start.
        let winds = chain(&[(44_100, 2, Some(60_000)), (44_100, 2, Some(20_000))], ask(Dir::Forward, 50_000, 4.0)).unwrap();
        assert_eq!(chain_map(&winds), vec![(0, 0, 50_000), (2_500, 0, 60_000), (2_500, 1, 0), (7_500, 1, 20_000)]);
    }

    #[test]
    fn a_chain_planned_at_the_streams_rate_is_the_one_its_tracks_own_rates_make() {
        // Up to 64 tracks a hold (0.9.14-2), each read only as it is reached: planned at the stream's rate, then made
        // again at its own; the same length and map as planned from the start at its own, and all of it sounds.
        for accel in [None, Some(Accel { period_s: 1.5, max: 64.0, audio: None, brake: None })] {
            let a = Ask { accel, ..ask(Dir::Back, 10_000, 2.0) };
            let own = chain(&[(44_100, 2, Some(60_000)), (48_000, 1, Some(30_000))], a).unwrap();
            let planned = chain(&[(44_100, 2, Some(60_000)), (44_100, 2, Some(30_000))], a).unwrap();
            let made = planned[1].at_rate(48_000);
            assert_eq!(made.data(), own[1].data());
            assert_eq!(made.map(), own[1].map());
            let sound = |w: &Wind| {
                let mut out = Vec::new();
                w.sound(&mut Tone { hz: 440.0, secs: 30.0 }, &mut Shaper::new(2), |x| {
                    out.extend_from_slice(&x);
                    true
                })
                .unwrap();
                out
            };
            assert_eq!(sound(&made).len() as u64, own[1].data());
        }
    }

    #[test]
    fn a_hold_speeds_up_doubling_to_its_top_and_on_through_the_queue() {
        let a = Accel { period_s: 1.5, max: 64.0, audio: None, brake: None };
        // From 8 ×: 64 × after three doublings (4.5 s); the way and its inverse agree.
        assert!((a.top_at(8.0) - 4.5).abs() < 1e-9);
        for t in [0.5, 3.0, 4.5, 9.0] {
            assert!((a.when(8.0, a.gone(8.0, t)) - t).abs() < 1e-6, "{t}");
        }
        // Five minutes back from 300 s at 8 × doubling every 1.5 s: far shorter than at a steady 8 × (37.5 s).
        let w = Wind::new(44_100, 2, Some(600_000), Ask { accel: Some(a), ..ask(Dir::Back, 300_000, 8.0) }).unwrap();
        let secs = w.ms() / 1000.0;
        assert!((secs - a.when(8.0, 300.0)).abs() < 0.01, "{secs}");
        assert!(secs < 10.0, "{secs}");
        assert_eq!(w.map(), vec![(0, 300_000), ((secs * 1000.0).round() as u64, 0)]);
        // It sounds through, faster as it goes (the pyramid's higher levels), and as long as asked.
        let mut wav = Vec::new();
        w.render(&mut Tone { hz: 40.0, secs: 600.0 }, |b| { wav.extend_from_slice(&b); true }).unwrap();
        assert_eq!(wav.len() as u64, w.bytes());
        // Through the queue the speed carries on: the next track at the speed the first ended with.
        let winds = chain(&[(44_100, 2, Some(600_000)), (44_100, 2, Some(300_000))], Ask { accel: Some(a), ..ask(Dir::Back, 10_000, 8.0) }).unwrap();
        let t1 = winds[0].ms() / 1000.0;
        assert!((winds[1].ms() / 1000.0 - (a.when(8.0, 310.0) - t1)).abs() < 0.01);
    }

    fn brake(v0: f64, to: f64, at: f64) -> Accel {
        Accel { period_s: 1.0, max: v0, audio: (v0 > 16.0).then_some((16.0, 350)), brake: Some(Brake { v0, to, secs: 1.5, at }) }
    }

    #[test]
    fn a_brake_is_a_raised_cosine_from_its_speed_to_its_end_speed() {
        let b = Brake { v0: 64.0, to: 1.0, secs: 1.5, at: 0.0 };
        assert!((b.v(0.0) - 64.0).abs() < 1e-9 && (b.v(1.5) - 1.0).abs() < 1e-9 && (b.v(9.0) - 1.0).abs() < 1e-9);
        // Its way: the mean of its ends over its length.
        assert!((b.w(1.5) - 32.5 * 1.5).abs() < 1e-9);
        let below = b.below(16.0).unwrap();
        assert!((b.v(below) - 16.0).abs() < 1e-9, "{below}");
        // Its stream from 0.3 s in: gone and when undo each other.
        let a = brake(64.0, 1.0, 0.3);
        for t in [0.0, 0.2, 0.7, 1.1] {
            assert!((a.when(4.0, a.gone(4.0, t)) - t).abs() < 1e-6, "{t}");
        }
    }

    #[test]
    fn a_brake_stream_carries_the_plan_to_its_aim_played_at_one() {
        // Let go at 64×, 0.3 s of the brake gone before its stream begins: 1.2 s of stream, from 100 s to the aim.
        let a = brake(64.0, 1.0, 0.3);
        let ask = Ask { accel: Some(a), ..ask(Dir::Forward, 100_000, 4.0) };
        let w = Wind::new(44_100, 2, Some(600_000), ask).unwrap();
        assert!((w.ms() - 1_200.0).abs() < 1.0, "{}", w.ms());
        let b = a.brake.unwrap();
        let aim = 100_000.0 + (b.w(1.5) - b.w(0.3)) * 1000.0;
        assert_eq!(w.map(), vec![(0, 100_000), (1_200, aim.round() as u64)]);
        // Heard in pieces until it slows below the audio top, then as it goes.
        let below = b.below(16.0).unwrap();
        let cut = (below * 44_100.0).round() as usize;
        assert!(w.pieces.len() > 2);
        assert_eq!(w.pieces.last().unwrap().own, (cut, w.frames));
        let mut out = Vec::new();
        assert!(w.render(&mut Tone { hz: 440.0, secs: 600.0 }, |x| { out.extend_from_slice(&x); true }).is_ok());
        assert_eq!(out.len() as u64, w.bytes());
    }

    #[test]
    fn a_brake_to_still_fades_as_it_stops() {
        // Back from 30 s at 8× to still: loud while it moves, silent at its end.
        let ask = Ask { accel: Some(brake(8.0, 0.0, 0.0)), ..ask(Dir::Back, 30_000, 4.0) };
        let w = Wind::new(44_100, 2, Some(60_000), ask).unwrap();
        let mut out = Vec::new();
        assert!(w.render(&mut Tone { hz: 440.0, secs: 60.0 }, |x| { out.extend_from_slice(&x); true }).is_ok());
        let level = |from: usize, to: usize| {
            let s: Vec<i16> = out[44 + from * 4..44 + to * 4].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            s.iter().map(|v| f64::from(*v).abs()).fold(0.0, f64::max)
        };
        let n = w.frames;
        assert!(level(n / 4, n / 4 + 2_000) > 1_000.0);
        assert!(level(n - 200, n) < 200.0, "{}", level(n - 200, n));
    }

    #[test]
    fn a_brake_goes_on_into_the_next_track_and_ends_at_its_aim() {
        // Let go at 16× a second before a 60 s track's end: (16 + 1) / 2 × 1.5 s = 12.75 s of way, 11.75 s into the next.
        let ask = Ask { accel: Some(brake(16.0, 1.0, 0.0)), ..ask(Dir::Forward, 59_000, 4.0) };
        let winds = chain(&[(44_100, 2, Some(60_000)), (44_100, 2, Some(120_000)), (44_100, 2, Some(30_000))], ask).unwrap();
        assert_eq!(winds.len(), 2, "no track past its aim");
        let map = chain_map(&winds);
        let (end, track, at) = *map.last().unwrap();
        assert_eq!((end, track), (1_500, 1));
        assert!((at as i64 - 11_750).abs() <= 1, "{at}");
    }

    #[test]
    fn a_flight_goes_on_past_what_is_heard_in_pieces() {
        // Heard at most at 16 ×, flying on to 128 ×, 350 ms pieces: 8 × reaches 16 × after one doubling (1.5 s).
        let flight = Accel { period_s: 1.5, max: 128.0, audio: Some((16.0, 350)), brake: None };
        let heard = Accel { period_s: 1.5, max: 16.0, audio: None, brake: None };
        let w = Wind::new(44_100, 2, Some(1_200_000), Ask { accel: Some(flight), ..ask(Dir::Back, 1_000_000, 8.0) }).unwrap();
        let plain = Wind::new(44_100, 2, Some(1_200_000), Ask { accel: Some(heard), ..ask(Dir::Back, 1_000_000, 8.0) }).unwrap();
        // Sooner at the edge than at a steady 16 ×, as long as its flight's way says.
        assert!(w.ms() < plain.ms() / 2.0, "{} {}", w.ms(), plain.ms());
        assert!((w.ms() / 1000.0 - flight.when(8.0, 1000.0)).abs() < 0.01);
        // Continuous up to 1.5 s, then a piece every 350 ms, each from where the flight has got.
        let p = &w.pieces;
        assert!(p.len() > 10);
        assert_eq!(p[0].own, (0, (1.5 * 44_100.0_f64).round() as usize));
        for q in &p[1..p.len() - 1] {
            assert_eq!(q.own.1 - q.own.0, (0.35 * 44_100.0_f64).round() as usize, "{q:?}");
            let half = (XFADE * 44_100.0 / 2.0) as usize;
            let at_own = q.top - 16.0 * (q.own.0 - q.from) as f64;
            let flown = 1_000.0 * 44_100.0 - flight.gone(8.0, q.own.0 as f64 / 44_100.0) * 44_100.0;
            assert!((at_own - flown).abs() < 2.0, "{at_own} {flown} {half}");
        }
        // Heard no faster than 16 ×.
        assert!((w.step(w.frames - 1) - 16.0).abs() < 1e-9);
        // Without a flight beyond what is heard, as before: one piece.
        assert_eq!(plain.pieces.len(), 1);
        let mut wav = Vec::new();
        w.render(&mut Tone { hz: 40.0, secs: 1_200.0 }, |b| { wav.extend_from_slice(&b); true }).unwrap();
        assert_eq!(wav.len() as u64, w.bytes());
    }

    #[test]
    fn a_rising_hold_keeps_its_loudness_and_tames_its_treble() {
        // A steady tone sped up: its loudness held near the first half second's, never more than the leveller allows.
        let mut s = Shaper::new(1);
        let quiet: Vec<f64> = (0..44_100).map(|i| 0.1 * (2.0 * PI * 100.0 * f64::from(i) / 44_100.0).sin()).collect();
        let mut first = vec![quiet.clone()];
        s.shape(&mut first, 4.0, 44_100.0);
        let loud: Vec<f64> = quiet.iter().map(|x| x * 4.0).collect();
        let rms = |x: &[f64]| (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt();
        let mut after = rms(&quiet) * 4.0;
        for _ in 0..8 {
            let mut c = vec![loud[..11_025].to_vec()];
            s.shape(&mut c, 4.0, 44_100.0);
            after = rms(&c[0]);
        }
        assert!((after / rms(&quiet) - 2.0).abs() < 0.1, "held down by its most: {}", after / rms(&quiet));
        // At 64 × the treble goes: a 10 kHz tone well down, a 500 Hz one kept.
        let tone = |hz: f64| -> Vec<f64> { (0..44_100).map(|i| 0.5 * (2.0 * PI * hz * f64::from(i) / 44_100.0).sin()).collect() };
        let through = |hz: f64| {
            let mut s = Shaper::new(1);
            let mut c = vec![tone(hz)];
            s.shape(&mut c, 64.0, 44_100.0);
            rms(&c[0][22_050..]) / rms(&tone(hz)[22_050..])
        };
        assert!(through(10_000.0) < 0.15, "{}", through(10_000.0));
        assert!(through(500.0) > 0.9, "{}", through(500.0));
    }

    #[test]
    fn it_winds_quieter_by_the_wind_volume() {
        let rms = |b: &[u8]| {
            let s: Vec<f64> = b[44..].chunks_exact(4).map(|f| f64::from(i16::from_le_bytes([f[0], f[1]])) / 32768.0).collect();
            (s.iter().map(|v| v * v).sum::<f64>() / s.len() as f64).sqrt()
        };
        let loud = Wind::new(44_100, 2, Some(60_000), ask(Dir::Back, 4_000, 1.0)).unwrap();
        let quiet = Wind::new(44_100, 2, Some(60_000), Ask { gain_db: -12.0, ..ask(Dir::Back, 4_000, 1.0) }).unwrap();
        let (l, q) = (rms(&wav(&loud, &mut Tone { hz: 200.0, secs: 60.0 })), rms(&wav(&quiet, &mut Tone { hz: 200.0, secs: 60.0 })));
        assert!((q / l - 0.251).abs() < 0.01, "{}", q / l);
    }
}
