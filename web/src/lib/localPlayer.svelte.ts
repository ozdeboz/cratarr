// The local player (decision 0022): Cratarr plays in the browser, gapless, through one continuous MSE stream. Each
// track comes from the server (/api/stream: FLAC repackaged as fragmented MP4, MP3 as it is; on a phone a lossless
// track as Opus in fragmented MP4, the owner 2026-10-24) and is appended to the
// same stream right after the one before: MP3 trimmed by its LAME header's encoder delay at the start and to its exact
// length at the end (measured to the sample with tools/gapless-lab), FLAC placed at its own sample positions. The next
// track is appended while the current one plays; what has been played is dropped from the buffer again. A track is
// read only as far as the playback needs (LEAD), so the browser's small audio buffer never has to hold one whole.
//
// The stream's time is mapped back to "which track, how far in" through the segments: one per appended track, from
// where its sample 0 sits in the stream. Skipping to an appended track moves within the stream (no gap, no reload);
// anything else (a jump back, a seek outside what is loaded) starts a fresh stream at that point.
//
// Inside the Android app the app plays instead, natively (decision 0031, amended), and the page can also drive another
// of Cratarr's players (Play On, `drive`): then every call below is handed to that player (app.svelte.ts, or the
// server's /api/players), what it reports is shown in `local`, and the stream here is not used.

import { playback } from './deviceLog';
import { cue } from './queueCue.svelte';
import { defaults, defaultsKnown, type Choices } from './mySettings.svelte';
import { inApp, onApp, toApp } from './app.svelte';
import { albumCoverUrl, getTrackGain, playerCommand, radioNext, windMap, type QueueItem, type TrackGain } from './api';
import { commandOf } from './player/commands';
import { prefs } from './prefs.svelte';
import { aimPainted, holdNow, ramping, rampBegins, rampEnds, rewindNow, tapeDir, windAsked } from './ramping.svelte';
import { easeInto, easing, readAt, shownAt, type Course, type Ease } from './deckCourse';
import { asksSignIn, toSignIn } from './reach.svelte';
import { consumed, rate, tapMs } from './windShape';
import { BANDCAMP } from './bandcampPreview';
import { firstFrame } from './mp3Frames';
import { DEVICE } from './deviceMusic.svelte';
import { RATE_FLOOR, RATE_TOP } from './rates';
import { rampSlot } from './player/rampSlot';
import { motionSlot } from './player/motionSlot';
import { armSlot } from './player/armSlot';
import { windBound } from './player/windBound';
import { afterLetGo, holdInReverse, knobFor, letGoTo, notSounding, SKIPPED } from './player/knob';
import { drivenHold, NO_DRIVEN_HOLD, NO_OWN_RAMP, NO_PENDING_NEXT, ownRamp, pendingNext, type DrivenHold, type OwnRamp, type PendingNext } from './player/tapeParts';
import { due, entered, NO_DEADLINES, type Deadlines } from './player/graces';
import { brakeLengthMs, brakeSpeed, brakeTravel, FLIGHT_TOP, RESTART_AFTER, HOLD_TOP, holdWind, PIECE_MS, pointWind, windsBy, type HoldWind, type Motion, type PointMotion, type StartStop, type Wind } from './startStop';
import { isPhone } from './device';
import { shuffled } from './shuffle';
import { toast } from './toast.svelte';

export type LocalTrack = {
  /** Its entry in a player's queue (decision 0033), once Cratarr or the app has given it one. */
  entry?: string;
  /** `next`: added by hand (Play next, Add to queue), not part of the album or playlist that plays (decision 0033's
   *  two lanes). `radio`: Radio's inserts, right after the own track they are like (and its hand-added run);
   *  `autoplay`: Autoplay's, after everything (decision 0036, amended). */
  lane?: 'next' | 'radio' | 'autoplay';
  /** Its place in the album or playlist as it was played, before any shuffle: turning shuffle off orders by it. */
  n?: number;
  source: string;
  id: string;
  name: string;
  artist: string | null;
  album: string | null;
  album_id: string | null;
  /** False for a track no playlist line can name (outside every library Cratarr sees). */
  addable?: boolean;
  /** Its length (s), where the list it came from said: the queue's lengths and when it ends. */
  duration_secs?: number | null;
  /** Who asked for it, a queue request accepted (0.10). */
  asked_by?: string;
};

export type Repeat = 'off' | 'one' | 'all';
/** A sleep timer: pause at `atMs` (epoch ms), or when the playing track ends. */
export type Sleep = { atMs: number | null; endOfTrack: boolean };
/** Autoplay's modes (decision 0036, amended); off is null. */
export type AutoplayMode = 'similar' | 'artist' | 'era' | 'random';
export const AUTOPLAY_MODES: readonly AutoplayMode[] = ['similar', 'artist', 'era', 'random'];
/** What Radio's inserts are like (decision 0036, amended): the track (its artist weighed double), its artist, its era. */
export type RadioBy = 'track' | 'artist' | 'era';
export const RADIO_BYS: readonly RadioBy[] = ['track', 'artist', 'era'];
/** How many Radio inserts after each own track: the choices offered, and the most the server keeps. */
export const RADIO_COUNTS: readonly number[] = [3, 5, 10];
export const RADIO_MAX = 10;
/** Autoplay while on: its mode; whole albums (`albums`); a playlist that ends starts again instead (`restart`); the
 *  tracks it stays close to when started from an item, else the queue's own (`seed`); the crate it plays inside
 *  ("Play as radio": only the crate's tracks), when it does. */
export type Autoplay = { mode: AutoplayMode; albums: boolean; restart: boolean; seed: string[]; within: string | null };

/** One appended track: its queue index, where its sample 0 sits in the stream (s), its length (s; null while
 *  unknown), and the length the server read from the file (s), shown while the exact one is unknown. */
/** `level`: the track's levelling, fixed once known (a queue change mid-track does not change it). */
type Segment = { index: number; start: number; length: number | null; told: number | null; level?: number };

export const local = $state({
  queue: [] as LocalTrack[],
  /** The playing track's index in `queue`; -1 when nothing is loaded. */
  index: -1,
  paused: true,
  /** The track left last, and how much of it was played then (0..1; wound to its end: 1): its waveform, still shown
   *  while the next one's comes in, keeps that colour (the owner, 2026-10-28: its ends looked unplayed). */
  left: null as { index: number; share: number } | null,
  /** Asked to play from rest and not sounding yet (its stream still coming): not yet playing, as the waveform shows it
   *  (the owner, 2026-10-28: only Start and stop shows the intention ahead of the truth). */
  starting: false,
  /** Seconds into the playing track. */
  position: 0,
  /** The playing track's length (s), when known. */
  duration: null as number | null,
  /** The last thing that failed, in plain words. */
  error: null as string | null,
  /** How far the playing track is turned down (dB, 0 or less): the level the player applies (this page's, or the app's
   *  `levelDb`); 0 with levelling off. The player's Level button and waveform show it (the owner, 2026-10-26). */
  levelDb: 0,
  /** Whether `levelDb` is the playing track's own level, not the guess for one not known yet (its gain still coming):
   *  the waveform waits for it before it shows the level (the owner, 2026-10-28: Output undone at Next). */
  levelKnown: true,
  /** What comes of the album or playlist plays shuffled (the player's shuffle switch). */
  shuffled: false,
  /** Repeat (decision 0033, amended): off, one (the playing track), all (the album or playlist). */
  repeat: 'off' as Repeat,
  /** The sleep timer of the player that plays elsewhere (the app, Cratarr's player), as it reports it; null when none
   *  is set or it fired. */
  sleep: null as Sleep | null,
  /** Radio (decision 0036, amended): while on, `radioCount` tracks like each of the queue's own tracks (`radioBy`: the
   *  track, its artist or its era) play right after it. Choice and count stay while off. */
  radio: false,
  radioBy: 'track' as RadioBy,
  radioCount: 5,
  /** Radio's and Autoplay's picks leave the owner's favourites out (the More music sheet's "Ignore favourites"). */
  skipFavourites: false,
  /** Autoplay, null while off: the music goes on when the queue runs out, chosen from the library. */
  autoplay: null as Autoplay | null,
  /** What the queue was started from ("Chill", an album), for the queue sheet's "Playing from"; null when unknown. */
  from: null as string | null,
});

const MP4 = 'audio/mp4; codecs="flac"';
const OPUS = 'audio/mp4; codecs="opus"';
/** AAC in MP4: asked of the browser for the log only (Cratarr makes no AAC: its encoder's licence; ALAC and MP3 instead). */
const AAC = 'audio/mp4; codecs="mp4a.40.2"';
const MPEG = 'audio/mpeg';
/** Keep this much ahead in the stream before fetching the next track (s). */
const AHEAD = 30;
/** Within a track, read no further than this ahead of the playing position (s): the rest of the fetch waits for the
 *  playback, so a long track never has to fit the browser's buffer whole (Chrome takes 12 MiB of audio, about two
 *  minutes of FLAC; on a phone 5, 2 or even 1 MiB). */
const LEAD = 60;
/** Bytes appended at a time (a phone's whole audio buffer can be 1 MiB). */
const SLICE = 256 * 1024;
/** Drop played audio older than this behind the playing position (s). */
const BEHIND = 30;
export { RESTART_AFTER } from './startStop';

// The stream this browser offers: MediaSource, or on iPhone and iPad (Safari 17.1 on) only ManagedMediaSource, which
// works the same but lets the browser say when to fetch (startstreaming, endstreaming) and drop buffered audio itself
// (bufferedchange). Neither: the local player is not offered.
const Managed = typeof window === 'undefined' ? undefined : (window as unknown as { ManagedMediaSource?: typeof MediaSource }).ManagedMediaSource;
const Source = Managed ?? (typeof MediaSource === 'undefined' ? undefined : MediaSource);
/** Whether this browser can play here at all (FLAC or MP3 in its stream), or the app plays for it. */
export const canPlayHere = inApp || (!!Source && (Source.isTypeSupported(MP4) || Source.isTypeSupported(MPEG)));
export { isPhone };
/** Lossless tracks come as Opus: on a phone that plays it, unless it keeps lossless (Settings › This browser). About a
 *  sixth of the data, and far less for the phone to fetch and hold. */
const asOpus = () => isPhone && !prefs.phoneLossless && !!Source?.isTypeSupported(OPUS);
/** The forms a lossless track can be asked in, the browser's best first (the owner, 2026-10-26: bet on the shape, the
 *  browser picks): Opus first on a phone that keeps no lossless (above); the original where FLAC in MP4 is taken; Opus;
 *  ALAC in MP4; MP3 (?as=alac, ?as=mp3, made for iOS, whose player takes neither FLAC nor Opus in MP4). A lossy
 *  track comes as it is whatever is asked. */
type Form = 'original' | 'opus' | 'alac' | 'mp3';
const ALAC = 'audio/mp4; codecs="alac"';
function losslessForms(): Form[] {
  if (!Source) return ['original'];
  const out: Form[] = [];
  if (asOpus()) out.push('opus');
  if (Source.isTypeSupported(MP4)) out.push('original');
  if (Source.isTypeSupported(OPUS) && !out.includes('opus')) out.push('opus');
  if (Source.isTypeSupported(ALAC)) out.push('alac');
  if (Source.isTypeSupported(MPEG)) out.push('mp3');
  return out.length ? out : ['original'];
}
/** A track's form after a fallback: how many places down the list it went (one a time, on a stall with nothing in or
 *  the element's error; logged). */
const demoted = new Map<string, number>();
const formOf = (t: { source: string; id: string }): Form => {
  const forms = losslessForms();
  return forms[Math.min(demoted.get(`${t.source}/${t.id}`) ?? 0, forms.length - 1)];
};
/** One place down for this track, when there is one: whether it went. */
function demote(t: { source: string; id: string }, why: string): boolean {
  const key = `${t.source}/${t.id}`;
  const forms = losslessForms();
  const at = demoted.get(key) ?? 0;
  if (at + 1 >= forms.length) return false;
  demoted.set(key, at + 1);
  playback('warn', 'form fallback', { track: t.id, from: forms[at], to: forms[at + 1], why });
  return true;
}
/** The form a lossless track is asked in here, null for the original (lib/offline asks the same). */
export function losslessAs(): Exclude<Form, 'original'> | null {
  const f = losslessForms()[0];
  return f === 'original' ? null : f;
}
/** The smaller copy this browser plays: Opus where taken; null where not. */
export const playsSmaller = (): 'opus' | null => (Source?.isTypeSupported(OPUS) ? 'opus' : null);
// What this browser's player takes, once per page in its log (the iPhone's tracks that would not play, 2026-10-26).
if (Source)
  playback('info', 'formats', {
    managed: !!Managed,
    flac: Source.isTypeSupported(MP4),
    opus: Source.isTypeSupported(OPUS),
    aac: Source.isTypeSupported(AAC),
    alac: Source.isTypeSupported('audio/mp4; codecs="alac"'),
    mp3: Source.isTypeSupported(MPEG),
  });

const audio = typeof Audio === 'undefined' || !canPlayHere || inApp ? null : new Audio();
// Safari opens a managed stream only for an element that cannot be sent to AirPlay (the stream cannot go there).
if (audio && Managed) audio.disableRemotePlayback = true;
let ms: MediaSource | null = null;
let sb: SourceBuffer | null = null;
let mime: string | null = null;
let segments: Segment[] = [];
/** Stream time where the next appended track's sample 0 goes. */
let streamEnd = 0;
/** Bumped by every fresh stream; work for an older one stops. */
let generation = 0;
/** Where the latest fresh stream begins (queue track, s into it): a handover there waits for it, never fetches again. */
let streamFrom: { index: number; from: number } | null = null;
let fetching: AbortController | null = null;
let appending = false;
/** The queue index being fetched and appended (while `appending`). */
let appendingIndex = -1;
/** The last queue index appended to the stream. */
let lastAppended = -1;
/** The queue index `local.error` is about: it goes once a later track plays. */
let errorIndex = -1;

// --- The tape engine's one state (decision 0075; the owner, 2026-10-27: "spread... fix it") ---
// Everything that moves the tape, in one place, so no two parts of it can disagree: what a ramp has committed to and how
// far it has got, whether it is the tape's own (quiet: a hold's stop, a knob's turn, a wind-up after a wind) or a Start
// and stop effect, a wind under way and where it stands, a hold, what waits for its moment. Its ramp and its motion
// are one value each (lib/player/rampSlot, motionSlot), so what may not hold together cannot; its passing states each
// have a deadline, set as they are entered (settleTape, lib/player/graces); resetTape clears every transient part at
// once where something new begins (a jump, a new queue, a stop).
/** The ramp under way, one value (lib/player/rampSlot): what it committed to, where it holds the counter. */
const rampNow = rampSlot();
/** The tape's motion, one value (lib/player/motionSlot): a wind here, a plain hold, or a driven player's glide. */
type PlainHold = { dir: 1 | -1; speeding: boolean };
type Glide = { from: number; to: number; at: number; span: number };
const motionNow = motionSlot<Winding, PlainHold, Glide>();
/** Where a wind's counter stands; only a wind has one (`tape.rewindPos`). */
let windPos: number | null = null;
/** `rested`: its wind never sounded, the tape resting where it stood (let go, it plays on from there). */
type Hold = { dir: 1 | -1; released: boolean; rested?: { index: number; pos: number } };
let holdState: Hold | null = null;
const quiet = { pause: false, until: 0 };
/** The tape's own ramp, a Next waiting for its moment, a hold on the player driven: each one value, changed by its own
 *  transitions only (lib/player/tapeParts). */
type TurnRamp = { mode: 'vinyl'; ms: number; stopMs: number };
let own: OwnRamp<TurnRamp> = NO_OWN_RAMP;
let nextKept: PendingNext = NO_PENDING_NEXT;
let drivenHeld: DrivenHold = NO_DRIVEN_HOLD;
/** The streams the tape asks for ahead (lib/player/armSlot): the wind to a point's, a hold's at its touch, the knob's
 *  backwards one at its turn. Asked anew, the one before is let go. */
const pointArms = armSlot<Armed>((a) => disarm(a));
const preArms = armSlot<{ dir: 1 | -1; armed: Armed; at: number }>((p) => disarm(p.armed));
const turnArms = armSlot<Armed>((a) => disarm(a));
/** The ramp and the motion tied (lib/player/windBound): a wind only inside its commitment. */
const bound = windBound(rampNow, motionNow, () => endRewind());
const tape = {
  /** What a ramp under way has committed to (paused or playing), shown at once on every device. A press of play or pause
   *  during one turns it round from where it has got to (decision 0075's first step). Null: no ramp. */
  get committed() {
    return rampNow.committed;
  },
  set committed(v: boolean | null) {
    // A wind lives only inside its commitment (to playing): a ramp ended outright while one sounds (a new queue, a Stop,
    // a seek during a ramp) ends the wind first, its clip stopped and its parts let go (endRewind), as its own end does.
    bound.commit(v);
    tapeChanged();
  },
  /** How far up the ramp under way has got (0: still or silent, 1: full speed and volume), where a press turns it. */
  rampAt: 1,
  /** A turn of the tape's own ramp: a vinyl's, over the tape's spin-up and Brake, whatever Start and stop says. */
  get turnRamp() {
    return own.turn;
  },
  /** A ramp the tape makes for itself (not a Start and stop effect): the player still shows playing through it. */
  get quiet() {
    return own.quiet;
  },
  /** The quiet pause under way, or the tape's own rest between two of its actions: shown as playing. */
  get quietPause() {
    return quiet.pause;
  },
  set quietPause(v: boolean) {
    quiet.pause = v;
    tapeChanged();
  },
  /** Until when that rest may stand (settle); past it, with nothing of the tape's running, it is over (a deadline of
   *  the tape's, lib/player/graces). */
  get quietUntil() {
    return quiet.until;
  },
  set quietUntil(v: number) {
    quiet.until = v;
    tapeChanged();
  },
  /** Where the counter stays while an effect's ramp sounds (stream time); null: where it plays. Only a ramp holds it. */
  get heldAt() {
    return rampNow.heldAt;
  },
  set heldAt(v: number | null) {
    rampNow.heldAt = v;
  },
  /** The wind sounding, or about to; null: none. One of the tape's motions (`motionNow`). */
  get rewinding() {
    return motionNow.rewinding;
  },
  set rewinding(v: Winding | null) {
    // A wind begun is committed to playing from its first moment (startWind says so itself a moment later).
    bound.wind(v);
    tapeChanged();
  },
  /** Where the counter stands while it sounds (s into the track): the wind's, so none once it has ended, however. */
  get rewindPos() {
    return motionNow.rewinding ? windPos : null;
  },
  set rewindPos(v: number | null) {
    windPos = v;
  },
  /** The wind's own fetch of where it goes: it does not end the wind. */
  rewindFetch: false,
  /** A hold under way (its press), before and while it winds: let go, it plays on. */
  get holding() {
    return holdState;
  },
  set holding(v: Hold | null) {
    holdState = v;
    tapeChanged();
  },
  /** A hold on a track that cannot be wound (Bandcamp's, the device's own). */
  get plainHold() {
    return motionNow.plainHold;
  },
  set plainHold(v: PlainHold | null) {
    motionNow.plainHold = v;
    tapeChanged();
  },
  /** The forward wind to a point being armed (a newer one replaces it). */
  get pointArmed() {
    return pointArms.now;
  },
  /** A hold's stream asked at the touch, for where the hold will start from. */
  get prearmed() {
    return preArms.now;
  },
  /** The knob's backwards stream asked at its turn, from where the brake will stand (no wait after the stop). */
  get turnArmed() {
    return turnArms.now;
  },
  /** Next pressed while a pause brakes: the next track, started up once the tape stands still. */
  get nextAfterPause() {
    return nextKept.afterPause;
  },
  /** Next pressed on the last track while Autoplay fetches: on into its first pick once it is in. */
  get nextWhenFilled() {
    return nextKept.whenFilled;
  },
  /** A hold's speed held back by the finger dragged down (0: none; 1: down to its starting speed): its wish. */
  dragWant: 0,
  /** A hold sent to the player driven, from its press until its wind has landed: its reported pauses not shown. */
  get heldThere() {
    return drivenHeld.held;
  },
  get heldLetGo() {
    return drivenHeld.letGo;
  },
  /** A wind on the player driven: the counter gliding between its reports. */
  get glide() {
    return motionNow.glide;
  },
  set glide(v: Glide | null) {
    motionNow.glide = v;
    tapeChanged();
  },
};
/** Something new begins (a jump, a new queue, a stop): every transient part of the tape cleared at once, so nothing of
 *  what went before lingers (a hold, a pending Next, an armed stream, a quiet rest, a ramp's hold on the counter). A
 *  wind under way and a ramp end through their own (endRewind, atOnce), which the callers run first. */
function resetTape() {
  pointArms.drop();
  preArms.drop();
  turnArms.drop();
  aim(null);
  Object.assign(tape, {
    holding: null,
    plainHold: null,
    dragWant: 0,
    quietPause: false,
    quietUntil: 0,
  });
  nextKept = NO_PENDING_NEXT;
  own = NO_OWN_RAMP;
}
/** The tape's passing states, each with its deadline (lib/player/graces): set as it is entered, the moment the tape
 *  changes; reached, a transition of its own. On the page's own player only, as its clock (`tick`) runs. */
let graces: Deadlines = NO_DEADLINES;
let settling = false;
/** The tape changed: what follows from it is settled once the change is done (its setters run in a row). */
function tapeChanged() {
  if (settling) return;
  settling = true;
  queueMicrotask(settleTape);
}
const said = new Map<string, number>();
const SAY_EVERY_MS = 10_000;
function sayOnce(what: string, fields: Record<string, unknown> = {}) {
  const now = performance.now();
  if (now - (said.get(what) ?? -Infinity) < SAY_EVERY_MS) return;
  said.set(what, now);
  playback('warn', `tape broke: ${what}`, fields);
}
/** What follows from the tape as it stands: the knob forward when nothing plays backwards; the passing states'
 *  deadlines entered or left; those reached, their transitions. (A wind without its commitment cannot be: the tape's
 *  setters keep it inside one.) */
function settleTape() {
  settling = false;
  if (!audio || local.index < 0 || handed()) return;
  const now = performance.now();
  const r = tape.rewinding;
  const knob = knobFor(tapeDir, r && { reverse: r.reverse, backTo: !!r.backTo });
  if (knob) {
    sayOnce('the knob backwards with nothing playing backwards');
    Object.assign(tapeDir, knob);
  }
  const free = tape.committed === null && !tape.rewinding;
  graces = entered(
    graces,
    {
      shownRamp: ramping.dir !== null && free,
      knobTurn: tapeDir.state === 'stopping' || tapeDir.state === 'returning',
      idleHold: !!tape.holding && free,
      quietRest: tape.quietPause && free ? tape.quietUntil : null,
    },
    now,
  );
  for (const k of due(graces, now)) {
    graces = { ...graces, [k]: null };
    if (k === 'shownRamp') {
      sayOnce('a ramp shown with none running', { dir: ramping.dir });
      rampEnds();
    } else if (k === 'knobTurn') {
      sayOnce('a turn of the knob stuck', { state: tapeDir.state });
      tapeDir.state = 'forward';
    } else if (k === 'idleHold') {
      sayOnce('a hold with nothing under way');
      tape.holding = null;
    } else tape.quietPause = false;
  }
}
// The shown ramp and the knob are the tape's too: their changes are the tape's.
$effect.root(() => {
  $effect(() => {
    void [ramping.dir, tapeDir.state];
    tapeChanged();
  });
});

// --- MP3: the LAME/Xing header (the measured rule: only the encoder delay precedes the first real sample) ---

/** `at`: where the first frame starts, after the ID3 tag (the tag, covers and all, never goes into the stream). */
type Mp3Head = { rate: number; delay: number; samples: number | null; at: number };

/** Parses the first frame of an MP3 for its sample rate and LAME gapless information; null when more bytes are needed. */
function mp3Head(b: Uint8Array): Mp3Head | null | 'invalid' {
  let i = 0;
  if (b.length < 10) return null;
  if (b[0] === 0x49 && b[1] === 0x44 && b[2] === 0x33) i = 10 + ((b[6] << 21) | (b[7] << 14) | (b[8] << 7) | b[9]);
  const tagEnd = i; // the ID3 tag can be large (embedded covers of several MB): the limit below counts from its end
  // A frame whose next frame follows it (lib/mp3Frames' firstFrame): never a sync found by chance.
  const at = firstFrame(b, i);
  if (at === null || at + 200 > b.length) return b.length > tagEnd + (1 << 22) ? 'invalid' : null; // wait for the first frame and its header
  i = at;
  const version = (b[i + 1] >> 3) & 3; // 3 = MPEG1, 2 = MPEG2, 0 = MPEG2.5
  const rates = version === 3 ? [44100, 48000, 32000] : version === 2 ? [22050, 24000, 16000] : [11025, 12000, 8000];
  const rate = rates[(b[i + 2] >> 2) & 3] ?? 44100;
  const mono = (b[i + 3] & 0xc0) === 0xc0;
  const side = version === 3 ? (mono ? 17 : 32) : mono ? 9 : 17;
  const x = i + 4 + side;
  const tag = String.fromCharCode(b[x], b[x + 1], b[x + 2], b[x + 3]);
  if (tag !== 'Xing' && tag !== 'Info') return { rate, delay: 0, samples: null, at: i };
  const flags = (b[x + 4] << 24) | (b[x + 5] << 16) | (b[x + 6] << 8) | b[x + 7];
  let p = x + 8;
  let frames: number | null = null;
  if (flags & 1) {
    frames = ((b[p] << 24) | (b[p + 1] << 16) | (b[p + 2] << 8) | b[p + 3]) >>> 0;
    p += 4;
  }
  if (flags & 2) p += 4;
  if (flags & 4) p += 100;
  if (flags & 8) p += 4;
  const isLame = String.fromCharCode(b[p], b[p + 1], b[p + 2], b[p + 3]) === 'LAME' || String.fromCharCode(b[p], b[p + 1], b[p + 2], b[p + 3]) === 'Lavc';
  const d = p + 21;
  const delay = isLame ? (b[d] << 4) | (b[d + 1] >> 4) : 0;
  const padding = isLame ? ((b[d + 1] & 0x0f) << 8) | b[d + 2] : 0;
  const perFrame = version === 3 ? 1152 : 576;
  return { rate, delay, samples: frames === null ? null : frames * perFrame - delay - padding, at: i };
}

// --- the stream ---

async function waitUpdate() {
  if (sb?.updating) await new Promise((r) => sb!.addEventListener('updateend', r, { once: true }));
}

/** Seconds of audio buffered from the playing position on. */
function ahead() {
  if (!audio || !sb) return 0;
  const t = audio.currentTime;
  const b = sb.buffered;
  for (let i = 0; i < b.length; i++) if (b.start(i) <= t + 0.1 && b.end(i) > t) return b.end(i) - t;
  return 0;
}

/** Seconds of audio buffered in all. */
function held() {
  if (!sb) return 0;
  let s = 0;
  for (let i = 0; i < sb.buffered.length; i++) s += sb.buffered.end(i) - sb.buffered.start(i);
  return s;
}

/** Resolves once the playback moves on (the element's next timeupdate), or at once when `signal` aborts (a new
 *  stream, a cut, a stop). A paused player keeps it waiting: nothing more is needed until it plays. */
function playbackMoves(signal: AbortSignal) {
  return new Promise<void>((resolve) => {
    if (signal.aborted || !audio) return resolve();
    const done = new AbortController();
    const go = () => {
      done.abort();
      resolve();
    };
    audio.addEventListener('timeupdate', go, { signal: AbortSignal.any([signal, done.signal]) });
    signal.addEventListener('abort', go, { signal: done.signal });
  });
}

/** Appends bytes when there is reason and room for them, as developer.chrome.com/blog/quotaexceedederror advises:
 *  no further than LEAD ahead of the playing position, and when the browser's buffer is full (QuotaExceededError)
 *  once played audio is dropped or, with none left to drop, once the playback has used up some of what is ahead.
 *  Holding back holds the fetch back too (its reader is not read), and the server's stream waits with it. */
async function appendBytes(bytes: Uint8Array<ArrayBuffer>, gen: number, signal: AbortSignal) {
  // In slices: the browser wants room for a whole append before it takes any of it.
  for (let at = 0; at < bytes.length; at += SLICE) if (!(await appendSlice(bytes.subarray(at, at + SLICE), gen, signal))) return;
}

/** One slice of `appendBytes`; false when the stream moved on meanwhile. */
async function appendSlice(bytes: Uint8Array<ArrayBuffer>, gen: number, signal: AbortSignal) {
  // Cut meanwhile (the queue changed after this track): nothing more of it goes in.
  const gone = () => gen !== generation || !sb || signal.aborted;
  for (;;) {
    while (!gone() && ahead() > LEAD) {
      await evict();
      await playbackMoves(signal);
    }
    if (gone()) return false;
    await waitUpdate();
    if (gone()) return false;
    try {
      sb!.appendBuffer(bytes);
      await waitUpdate();
      return true;
    } catch (e) {
      if ((e as DOMException).name !== 'QuotaExceededError') throw e;
    }
    const before = held();
    await evict(5);
    if (held() < before) continue;
    // Nothing behind to drop and next to nothing ahead: the browser takes less than these bytes, waiting won't help.
    if (ahead() < 1) throw new Error('The browser has no room for more audio.');
    await playbackMoves(signal);
  }
}

/** Drops played audio more than `keep` seconds behind the playing position. */
async function evict(keep = BEHIND) {
  if (!sb || !audio || !sb.buffered.length) return;
  const until = audio.currentTime - keep;
  if (until <= sb.buffered.start(0) + 1) return;
  await waitUpdate();
  try {
    sb.remove(sb.buffered.start(0), until);
    await waitUpdate();
  } catch {
    /* the stream moved on; nothing to drop */
  }
}

/** Fetches queue track `index` and appends it to the stream, starting `from` seconds into it (a seek). `whole`: asked
 *  from its top all the same, placed so that `from` plays now (a stream from a moment that did not say where it began). */
async function appendTrack(index: number, from: number, gen: number, whole = false) {
  const t = local.queue[index];
  if (!t) return;
  wantGain(index);
  appending = true;
  appendingIndex = index;
  const ctl = (fetching = new AbortController());
  /** This track's segment, once placed in the stream. */
  let placedSegment = null as Segment | null; // set inside place(), which TypeScript does not follow
  try {
    const query = new URLSearchParams();
    // An MP3 asked from its frame there, not whole from its top (662527cb: mp3_from=1; the server says where it
    // starts, x-start-ms). A server before it ignores the flag and sends it whole, as before.
    if (from > 0 && !whole) {
      query.set('from_ms', String(Math.floor(from * 1000)));
      query.set('mp3_from', '1');
    }
    const form = formOf(t);
    if (form !== 'original') query.set('as', form);
    const res = await fetch(`/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}${query.toString() ? `?${query}` : ''}`, {
      signal: ctl.signal,
    });
    if (!res.ok) {
      if (asksSignIn(res)) toSignIn();
      let text = '';
      try {
        text = (await res.json()).error ?? '';
      } catch {
        /* no words */
      }
      throw new Error(`${t.name || 'A track'}: ${text || 'it could not be loaded.'}`);
    }
    // Part of a file from a moment that does not say where it begins (x-start-ms): never taken as its start (0.10: a
    // Bandcamp preview's seek, so placed, came out short and was passed over). Asked again from its top instead.
    if (from > 0 && !whole && res.status === 206 && !res.headers.has('x-start-ms')) {
      playback('warn', 'stream from a moment without its start', { track: t.id, source: t.source, from });
      ctl.abort();
      return appendTrack(index, from, gen, true);
    }
    const type = res.headers.get('content-type') ?? '';
    const kind = !type.startsWith('audio/mp4') ? MPEG : type.includes('opus') ? OPUS : type.includes('alac') ? ALAC : type.includes('mp4a') || type.includes('aac') ? AAC : MP4;
    playback('info', 'stream', { track: t.id, source: t.source, status: res.status, type, kind, supported: !!Source?.isTypeSupported(kind), from, managed: !!Managed });
    if (!Source?.isTypeSupported(kind)) {
      // The next form down, where there is one: asked again at once.
      if (demote(t, `not taken: ${kind}`) && gen === generation) {
        ctl.abort();
        return appendTrack(index, from, gen, whole);
      }
      throw new Error(`${t.name}: this browser cannot play ${kind === MP4 ? 'FLAC' : kind === OPUS ? 'Opus' : kind === ALAC ? 'ALAC' : kind === AAC ? 'AAC' : 'MP3'} here.`);
    }
    const reader = res.body!.getReader();
    // The track's sample 0 goes to `start` in the stream (before the stream's 0 when starting mid-track).
    const start = streamEnd - from;
    let head = new Uint8Array(0);
    let placed = false;
    let length: number | null = null;
    const told = Number(res.headers.get('x-duration-ms')) / 1000 || null;
    const segment: Segment = { index, start, length: null, told };
    /** An MP3's encoder delay: its LAME header's, else as the stream says it (Cratarr's own MP3 form has no header and
     *  sends x-pre-skip, 0.9.8). */
    const delayOf = (h: Mp3Head) => h.delay || Number(res.headers.get('x-pre-skip')) || 0;
    /** An MP3 sent from a frame mid-track (mp3_from): that frame's first sample's time in the track (s), its encoder's
     *  delay already taken off; null where it came whole. Then no delay of its own to skip, its length the told one. */
    const startHeader = res.headers.get('x-start-ms');
    const sentFrom = kind === MPEG && startHeader !== null && Number.isFinite(Number(startHeader)) ? Number(startHeader) / 1000 : null;
    const placeMp3 = (h: Mp3Head) => place(h.rate, sentFrom !== null ? 0 : delayOf(h), sentFrom !== null ? null : h.samples);
    const place = async (rate: number, skip: number, samples: number | null) => {
      if (!sb) {
        sb = ms!.addSourceBuffer(kind);
        if (Managed) sb.addEventListener('bufferedchange', purged);
        const added = sb;
        added.addEventListener('error', () => playback('warn', 'buffer error', { index, mime: kind }));
        mime = kind;
      } else if (mime !== kind) {
        await waitUpdate();
        sb.changeType(kind);
        mime = kind;
      }
      await waitUpdate();
      // The track before can end mid-frame (an MP3's last bytes are no whole frame, or its fetch was cut): the parser
      // then still waits for the rest, and neither the mode nor the offset may change ("PARSING_MEDIA_SEGMENT").
      // abort() drops that partial frame, which lies past that track's end (appendWindowEnd) anyway. A stream that was
      // ended (the queue's last track was in; more were added since) must be reopened first: removing nothing does it.
      if (ms?.readyState === 'ended') sb.remove(ms.duration, Infinity);
      // Until every removal is done (this one, a cut's, played audio dropped): nothing below may run while one does.
      while (sb.updating) await waitUpdate();
      if (ms?.readyState === 'open') sb.abort();
      // FLAC's and Opus's fragments carry their own sample times (tfdt; after a seek they start at the frame holding
      // the sample asked for), so they run in segments mode with the offset at the track's sample 0 (Opus's: past its
      // encoder's delay, x-pre-skip). MP3 has no times of its own: sequence mode, the offset at its first decoded
      // sample (the encoder delay before sample 0).
      const mode: AppendMode = kind === MPEG ? 'sequence' : 'segments';
      if (sb.mode !== mode) sb.mode = mode;
      length = samples === null ? null : samples / rate;
      // An MP3 header that disagrees with the length Cratarr read is trusted no more than that length (one header said
      // 5:27 for 4:24; an early-stopping decode once said 7 s for 7:48): the track's own audio, once in, says where it
      // ends, so it is neither cut short nor followed by silence. FLAC's own sample count is exact.
      if (kind === MPEG && length !== null && told !== null && Math.abs(length - told) > 1.5) length = null;
      sb.appendWindowEnd = Infinity;
      sb.appendWindowStart = Math.max(0, streamEnd);
      // Never before its start (a place asked at or past its end: nothing of it plays, and what follows goes on).
      if (length !== null) sb.appendWindowEnd = Math.max(sb.appendWindowStart + 0.001, start + length);
      // At a stream's start an MP3's encoder delay would put its offset below 0. Lead (measure, 2026-10-26): the
      // iPhone's four MP3s that stalled all carry a LAME/Info header (a delay), the one that played none; iOS's
      // ManagedMediaSource may not take a negative offset in sequence mode. From 0 there (a few ms of the encoder's
      // silence at a fresh stream's start; joins between tracks keep their gapless trim), and what was set in the log.
      // Only the delay is let go. A track's own MP3 comes whole from its first frame, so a start mid-track (a resume, a
      // seek) keeps its offset below 0 and the window cuts what lies before (the tablet, 2026-10-27: resumed at 2:24, it
      // played from the top while its clock ran 2:24 ahead, to 3:20 of a 2:38 track); Cratarr's own MP3 form (it sends
      // x-pre-skip) is made from the place asked, its first frame there.
      // An MP3 sent from its frame there (mp3_from): its first sample where that frame lies in the track, the window
      // cutting the frame or two before the place asked.
      const made = kind !== MPEG ? 0 : sentFrom !== null ? sentFrom : res.headers.has('x-pre-skip') ? from : 0;
      const offset = start + made - skip / rate;
      sb.timestampOffset = kind === MPEG ? Math.max(Math.min(0, start + made), offset) : offset;
      if (kind === MPEG)
        playback('info', 'mp3 placed', { index, rate, skip, sentFrom, offset: Math.round(offset * 1000) / 1000, set: Math.round(sb.timestampOffset * 1000) / 1000, from: sb.appendWindowStart, to: Number.isFinite(sb.appendWindowEnd) ? Math.round(sb.appendWindowEnd * 10) / 10 : null });
      segment.length = length;
      segments.push(segment);
      placedSegment = segment;
      // Its level from its first sample.
      levelSchedule();
      placed = true;
    };
    if (kind !== MPEG) {
      const rate = Number(res.headers.get('x-sample-rate')) || 44100;
      const total = Number(res.headers.get('x-total-samples')) || null;
      await place(rate, Number(res.headers.get('x-pre-skip')) || 0, total);
    }
    for (;;) {
      const { value, done } = await reader.read();
      if (gen !== generation) {
        reader.cancel().catch(() => {});
        return;
      }
      if (done) break;
      let chunk = value as Uint8Array<ArrayBuffer>;
      if (!placed) {
        const joined = new Uint8Array(head.length + chunk.length);
        joined.set(head);
        joined.set(chunk, head.length);
        const h = mp3Head(joined);
        if (h === null) {
          head = joined;
          continue;
        }
        if (h === 'invalid') throw new Error(`${t.name} is not a playable MP3.`);
        await placeMp3(h);
        chunk = joined.subarray(h.at);
      }
      await appendBytes(chunk, gen, ctl.signal);
    }
    if (!placed && head.length) {
      const h = mp3Head(head);
      if (h && h !== 'invalid') {
        await placeMp3(h);
        await appendBytes(head.subarray(h.at), gen, ctl.signal);
      }
    }
    // A new stream, or the queue changed after this track (cutFrom): it no longer counts as appended.
    if (gen !== generation || ctl.signal.aborted) return;
    // A length the header did not give: what the stream now holds.
    if (segment.length === null && sb && sb.buffered.length) segment.length = sb.buffered.end(sb.buffered.length - 1) - start;
    // Never back from where it stood (the tablet, 2026-10-28: resumed at 6:10 of a 6:10 track, none of it was in, and
    // every track after it was placed at -6:10 and failed): a track with nothing past the place asked is passed over.
    const passedOver = start + (segment.length ?? 0) <= streamEnd;
    if (passedOver)
      // Which it was: a place asked at its very end (the page's), or a file shorter than its length says (the library's).
      playback('warn', 'track passed over', { index, track: t.id, from: Math.round(from * 10) / 10, told, why: told !== null && from >= told - 1 ? 'asked at its end' : 'shorter than told' });
    streamEnd = Math.max(streamEnd, start + (segment.length ?? 0));
    lastAppended = index;
    if (index === local.queue.length - 1 && ms?.readyState === 'open') {
      await waitUpdate();
      ms.endOfStream();
    }
  } catch (e) {
    if ((e as DOMException).name === 'AbortError' || gen !== generation) return;
    playback('warn', 'track failed', { track: t.id, source: t.source, error: e instanceof Error ? `${e.name}: ${e.message}` : String(e), ms: ms?.readyState ?? null, sb: !!sb, mime });
    local.error = e instanceof Error ? e.message : String(e);
    errorIndex = index;
    // Skip what cannot play, as a player does, unless it was the only thing.
    if (index < local.queue.length - 1) {
      lastAppended = index;
      // Placed already (it failed midway): that segment becomes the empty one, not a second segment for the track.
      if (placedSegment) placedSegment.length = 0;
      else segments.push({ index, start: streamEnd, length: 0, told: null });
    }
  } finally {
    // Not when a newer append is under way (this one was cut).
    if (gen === generation && fetching === ctl) appending = false;
  }
}

/** Starts a fresh stream at queue track `index`, `from` seconds in. */
async function fresh(index: number, from = 0, play = true) {
  if (!audio) return;
  if (tape.rewinding && !tape.rewindFetch) endRewind();
  // A position at or past the track's end (a resume Cratarr reckoned on while the player was away; it once asked for
  // 36 minutes into a track of 2:23, 2026-10-24) starts the track from the top: the stream cannot begin past its end.
  const length = local.queue[index]?.duration_secs;
  if (!(from >= 0) || (length && from >= length - 1)) from = 0;
  const gen = ++generation;
  streamFrom = { index, from };
  fetching?.abort();
  // Its own track's append is under way from here (its source opening first): the tick appends no other meanwhile. A
  // wind readying this stream keeps the counter on the track it winds (prepare), which made the tick append this one a
  // second time into the same buffer (the owner's tablet, 2026-10-28: "still processing an 'appendBuffer'").
  appending = true;
  segments = [];
  streamEnd = 0;
  lastAppended = index - 1;
  sb = null;
  mime = null;
  local.index = index;
  local.position = from;
  local.error = null;
  wantGain(index);
  wantGain(index + 1);
  ms = new Source!();
  const url = URL.createObjectURL(ms);
  audio.src = url;
  playback('info', 'fresh', { index, track: local.queue[index]?.id ?? null, from: Math.round(from * 10) / 10, play });
  // From rest (not a track run into while playing on, nor the tape's own handover): not playing until it sounds.
  if (play && local.paused && !tape.rewindFetch) local.starting = true;
  watchSource(ms);
  if (play) audio.play().catch(refused);
  await new Promise((r) => ms!.addEventListener('sourceopen', r, { once: true }));
  // The element holds the stream now; the URL is no longer needed (each seek or jump makes a new one).
  URL.revokeObjectURL(url);
  if (gen !== generation) return;
  await appendTrack(index, from, gen);
}

/** Whether the browser wants audio fetched now: always, except a managed stream between its endstreaming and
 *  startstreaming (the tick then waits; what is being fetched finishes). */
function streaming() {
  return !Managed || (ms as unknown as { streaming?: boolean } | null)?.streaming !== false;
}

/** A managed stream's browser dropped buffered audio (bufferedchange). Behind the playing position that is what the
 *  player does itself; ahead of it the track it was in is fetched again: the one playing from where it is (a fresh
 *  stream), a later one by cutting the stream there, after which the tick appends it again. */
function purged() {
  if (!audio || !sb) return;
  const t = audio.currentTime;
  let end = t; // where the audio that plays now runs out
  for (let i = 0; i < sb.buffered.length; i++) if (sb.buffered.start(i) <= t + 0.1) end = Math.max(end, sb.buffered.end(i));
  if (end >= streamEnd - 0.5) return; // all appended audio ahead is still there
  const seg = segments.find((s) => s.start + (s.length ?? 0) > end + 0.5);
  if (!seg) return;
  if (seg.index <= local.index) fresh(local.index, local.position, !audio.paused);
  else cutFrom(seg.index);
}

/** Which segment the stream is in now, and how far into its track. */
function where(t = audio?.currentTime ?? 0): { seg: Segment; offset: number } | null {
  if (!audio) return null;
  for (let i = segments.length - 1; i >= 0; i--) {
    const s = segments[i];
    if (t >= s.start - 0.001) return { seg: s, offset: t - s.start };
  }
  return segments[0] ? { seg: segments[0], offset: 0 } : null;
}

// The clock: the playing track and position, the next track fetched ahead, played audio dropped, the lock screen kept
// up to date. Four times a second is plenty for the display; the audio itself runs on its own.
let lastShown = -1;
function tick() {
  // Played elsewhere (another of Cratarr's players): this page's own, idle audio says nothing about it.
  if (!audio || local.index < 0 || handed()) return;
  // A ramp under way has committed to where it goes (lib's Start and stop): that is what shows, everywhere; the tape's
  // own stop (quiet) shows playing.
  // The tape's deadlines reached (lib/player/graces): their transitions, at the clock's moments.
  settleTape();
  local.paused = tape.quietPause ? false : (tape.committed ?? audio.paused);
  const w = where(tape.heldAt ?? audio.currentTime);
  if (w && tape.rewinding) local.position = tape.rewindPos ?? local.position;
  else if (w) {
    if (w.seg.index !== local.index) {
      // Repeat one: the stream went on to the next track by itself; back to the start of this one. (Next and previous
      // set the index themselves, so they still move on.)
      if (local.repeat === 'one' && w.seg.index === local.index + 1) return jump(local.index);
      local.index = w.seg.index;
      nextRound();
      fill();
    }
    local.position = tape.rewindPos ?? Math.max(0, w.offset);
    local.duration = w.seg.length ?? w.seg.told;
  }
  if (local.error && local.index > errorIndex && !audio.paused) local.error = null;
  if (local.index !== lastShown) {
    lastShown = local.index;
    showOnLockScreen();
  }
  const gen = generation;
  if (!appending && lastAppended >= local.index && lastAppended < local.queue.length - 1 && streamEnd - audio.currentTime < AHEAD && streaming()) {
    appendTrack(lastAppended + 1, 0, gen);
  }
  if (!appending) evict();
  if ('mediaSession' in navigator && local.duration && Number.isFinite(local.duration)) {
    try {
      navigator.mediaSession.setPositionState({ duration: local.duration, position: Math.min(local.position, local.duration), playbackRate: 1 });
    } catch {
      /* not supported */
    }
  }
}
// Levelling (decision 0035): this browser's player plays every track about as loud as the next, by its ReplayGain (the
// album's while an album plays in its own order, else the track's), never past its peak nor above full volume, a
// typical gain where none is known (or not yet). Through a gain node, set as each track of the stream begins (the
// stream holds the tracks one after the other) and then kept for that track; each track's gain is asked for as the
// track is fetched, so it is there when the track begins. The node goes between the element and the speakers only
// once its audio context runs (a browser starts one only after a press): an element routed into a context that cannot
// start plays silence. Not in Safari (the managed stream's browser): WebKit suspends an audio context when the page
// goes to the background or the screen locks, which would silence the element routed into it; it plays unlevelled.
const UNKNOWN_DB = -6;
/** A track's level while its gain is asked for, or never measured: typical, or as it is (Level's sheet). */
const unknownLevel = () => (prefs.levelUnknown === 'untouched' ? 1 : 10 ** (UNKNOWN_DB / 20));
const canLevel = !!audio && !Managed && typeof AudioContext !== 'undefined';
const gains = new Map<string, TrackGain | null>();
let level: { ctx: AudioContext; gain: GainNode; source: MediaElementAudioSourceNode } | null = null;
/** An audio context made, not yet running. */
let starting: AudioContext | null = null;

/** `tap`: the route for the visuals' tap (`audioTap`), made even where levelling is off (its gain then stays 1). */
function levelRoute(tap = false) {
  if (level || starting || !canLevel || (!prefs.levelling && !tap)) return;
  let ctx: AudioContext;
  try {
    ctx = new AudioContext();
  } catch {
    return;
  }
  starting = ctx;
  // resume() answers once the context runs: at once after a press, else at the next one.
  ctx.resume().then(
    () => {
      starting = null;
      if (level || ctx.state !== 'running' || !audio) return void ctx.close();
      try {
        const gain = ctx.createGain();
        const source = ctx.createMediaElementSource(audio);
        source.connect(gain).connect(ctx.destination);
        level = { ctx, gain, source };
      } catch {
        return void ctx.close();
      }
      // Suspended by the browser while playing: the element would fall silent.
      ctx.addEventListener('statechange', () => {
        if (ctx.state === 'suspended' && audio && !audio.paused) void ctx.resume();
      });
      levelSchedule();
    },
    () => {
      starting = null;
      void ctx.close();
    },
  );
}

/** This page's own audio as Web Audio has it, for the visuals' live analysis (decision 0046): the levelling's route, made
 *  now where levelling is off (at no change of level; an element can feed only one route). Null while it is being made
 *  (an audio context starts after a press: the visuals open on one), and where there is nothing to tap: Safari (the
 *  managed stream; its audio context would silence the music in the background, see above), and while another player
 *  plays (the app's, or one of Cratarr's). */
export function audioTap(): { context: AudioContext; node: AudioNode } | null {
  if (!canLevel || handed()) return null;
  if (!level) levelRoute(true);
  return level && level.ctx.state === 'running' ? { context: level.ctx, node: level.source } : null;
}

/** Asks for a queue place's gain, once per track. */
function wantGain(index: number) {
  const t = local.queue[index];
  if (!t || !canLevel || !prefs.levelling) return;
  const key = `${t.source}/${t.id}`;
  if (gains.has(key)) return;
  gains.set(key, null);
  getTrackGain(t.source, t.id)
    .then((g) => gains.set(key, g), () => gains.set(key, {}))
    .finally(levelSchedule);
}

/** A queue place's level (0 to 1); null while its gain is being asked for. */
function levelOf(index: number): number | null {
  const t = local.queue[index];
  if (!t) return 1;
  wantGain(index);
  const g = gains.get(`${t.source}/${t.id}`);
  if (!g) return null;
  const sameAlbum = (j: number) => !!t.album_id && local.queue[j]?.album_id === t.album_id && !local.queue[j]?.lane;
  const albumWise = !local.shuffled && (sameAlbum(index - 1) || sameAlbum(index + 1));
  // The server's own rule where it says it (0.9.8: one rule, here and in the app); not measured (null): as a
  // typical track, or as it is (Level's sheet).
  const down = albumWise && g.albumDown !== undefined ? g.albumDown : g.trackDown;
  if (down !== undefined) return down === null ? unknownLevel() : Math.min(1, 10 ** (down / 20));
  const gainDb = (albumWise ? g.albumGain : undefined) ?? g.trackGain ?? (prefs.levelUnknown === 'untouched' ? 0 : UNKNOWN_DB);
  const peak = (albumWise ? g.albumPeak : undefined) ?? g.trackPeak ?? 1;
  return Math.min(10 ** (gainDb / 20), peak > 0 ? 1 / peak : 1, 1);
}

/** A segment's level: its track's, kept once known; the typical one until then. */
function segmentLevel(s: Segment): number {
  if (!prefs.levelling) return 1;
  if (s.level === undefined) {
    const l = levelOf(s.index);
    if (l === null) return unknownLevel();
    s.level = l;
  }
  return s.level;
}

/** What the gain node was last told: the playing segment and its level, the next track's (null: none yet, or paused)
 *  and when it starts (on the audio context's clock). */
let told: { seg: Segment | undefined; level: number; next: number | null; at: number } | null = null;
/** A change within a playing track glides (about 0.15 s): a step there could click. Elsewhere (a new track, a seek,
 *  before any sound) the sound jumps anyway, and it is a step. */
const GLIDE = 0.05;
/** Levelling switched on or off: the playing track turns to its new level over a second, never at once (the owner,
 *  2026-10-27: the switch's hard edges). */
const SWITCH_S = 1;
let switched = false;

/** The playing track's level now, the next one's from where it starts in the stream. Only what changed is told
 *  again: cancelling a glide under way would jump back. */
function levelSchedule() {
  if (!level || !audio) return;
  const p = level.gain.gain;
  const t = level.ctx.currentTime;
  const now = audio.currentTime;
  const cur = [...segments].reverse().find((s) => s.start <= now);
  const next = segments.find((s) => s.start > now);
  const f = cur ? segmentLevel(cur) : prefs.levelling ? unknownLevel() : 1;
  local.levelDb = f > 0 && f < 1 ? Math.round(20 * Math.log10(f) * 10) / 10 : 0;
  local.levelKnown = !prefs.levelling || (!!cur && cur.level !== undefined);
  // Paused: the stream's clock stands still while the context's runs on, so no next step is set.
  const nf = next && !audio.paused ? segmentLevel(next) : null;
  const at = next ? t + (next.start - now) : 0;
  if (told?.level !== f || told.seg !== cur) {
    // As when a track's gain comes late, or the switch is flipped.
    const within = !!cur && !audio.paused && now > cur.start + 0.3 && (!told || told.seg === cur);
    if (within) {
      // From where it is now.
      if (typeof p.cancelAndHoldAtTime === 'function') p.cancelAndHoldAtTime(t);
      else {
        p.cancelScheduledValues(t);
        p.setValueAtTime(p.value, t);
      }
      // The switch: even in decibels over a second (an exponential ramp), else the short glide of a late gain.
      if (switched && p.value > 0 && f > 0) p.exponentialRampToValueAtTime(f, t + SWITCH_S);
      else p.setTargetAtTime(f, t, GLIDE);
    } else {
      p.cancelScheduledValues(t);
      p.setValueAtTime(f, t);
    }
  } else if (told.next !== nf || (nf !== null && Math.abs(told.at - at) > 0.02)) {
    // Only the next step moved (it comes, goes, or the clocks drifted apart): what lies ahead, again.
    p.cancelScheduledValues(t + 0.001);
  } else return;
  switched = false;
  if (nf !== null) p.setValueAtTime(nf, at);
  told = { seg: cur, level: f, next: nf, at };
}

/** Levelling on or off for this browser's player, the playing track too. */
export function setLevelling(on: boolean) {
  prefs.levelling = on;
  switched = true;
  if (!on) local.levelDb = 0;
  // The switch is a press: an audio context can start now.
  if (audio && !audio.paused) levelRoute();
  levelSchedule();
}

if (audio) {
  audio.addEventListener('play', () => {
    levelRoute();
    void level?.ctx.resume();
    levelSchedule();
  });
  audio.addEventListener('seeked', levelSchedule);
  audio.addEventListener('pause', levelSchedule);
  audio.addEventListener('timeupdate', levelSchedule);
}

/** A play() the browser refused (Safari on iOS: not from a press), in the log; nothing on screen said so. */
function refused(e: unknown) {
  playback('warn', 'play refused', { error: e instanceof Error ? `${e.name}: ${e.message}` : String(e), index: local.index });
}
/** A stream's own events, in the log. */
function watchSource(m: MediaSource) {
  for (const ev of ['sourceended', 'sourceclose', 'startstreaming', 'endstreaming'])
    m.addEventListener(ev, () => playback('info', ev, { index: local.index, ms: m.readyState }));
}
// What the element itself says (the owner, 2026-10-26: tracks that silently do not play on an iPhone): its errors and
// stalls as warnings, its waits as notes.
if (audio) {
  const el = audio;
  // A track the browser said it takes but plays nothing of (the element's error, or stalled with nothing in): once
  // more, in the next form down (the owner, 2026-10-26: the browser picks, and a wrong yes costs a moment, not silence).
  const fallBack = (why: string) => {
    const t = local.queue[local.index];
    if (t && !handed() && demote(t, why)) void fresh(local.index, local.position, !local.paused);
  };
  el.addEventListener('error', () => {
    playback('warn', 'element error', { code: el.error?.code ?? null, message: el.error?.message ?? '', index: local.index });
    fallBack(`element error ${el.error?.code ?? ''}`);
  });
  el.addEventListener('stalled', () => {
    const buffered = el.buffered.length ? Math.round(el.buffered.end(el.buffered.length - 1) * 10) / 10 : 0;
    playback('warn', 'stalled', { index: local.index, at: Math.round(el.currentTime * 10) / 10, buffered });
    if (buffered === 0) fallBack('stalled with nothing in');
  });
  el.addEventListener('waiting', () => playback('info', 'waiting', { index: local.index, at: Math.round(el.currentTime * 10) / 10 }));
  el.addEventListener('playing', () => {
    local.starting = false;
    // Sounding: no rest of the tape's to show any more.
    if (tape.committed === null && !tape.rewinding) tape.quietPause = false;
    playback('info', 'playing', { index: local.index, track: local.queue[local.index]?.id ?? null });
  });
  el.addEventListener('pause', () => (local.starting = false));
}

if (audio) {
  setInterval(tick, 250);
  audio.addEventListener('play', tick);
  audio.addEventListener('pause', tick);
  audio.addEventListener('ended', () => {
    local.paused = true;
  });
}

function showOnLockScreen() {
  if (!('mediaSession' in navigator)) return;
  const t = local.queue[local.index];
  if (!t) return;
  lockScreenActions();
  // The album's cover as the page shows it (in the browser's cache already), else the track's; given again once it is
  // in, as iOS keeps a blank square for artwork that was not there yet when given (the owner's iPhone, 2026-10-27).
  const src = new URL(t.album_id ? albumCoverUrl(t.source, t.album_id) : `/api/cover/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}`, location.href).href;
  const give = () => {
    if (local.queue[local.index] !== t) return;
    navigator.mediaSession.metadata = new MediaMetadata({ title: t.name, artist: t.artist ?? '', album: t.album ?? '', artwork: [{ src }] });
  };
  give();
  const img = new Image();
  img.onload = give;
  img.src = src;
}

/** What the lock screen's buttons do. Next and previous, not iOS's ±10 s that it shows unless seeking back and forth
 *  are said to be none (the owner's iPhone, 2026-10-27); given again with each track, as WebKit may drop them. */
function lockScreenActions() {
  const ms_ = navigator.mediaSession;
  ms_.setActionHandler('play', () => rampIn());
  ms_.setActionHandler('pause', () => void pauseRamped());
  ms_.setActionHandler('nexttrack', () => next());
  ms_.setActionHandler('previoustrack', () => previous());
  for (const a of ['seekbackward', 'seekforward'] as const) {
    try {
      ms_.setActionHandler(a, null);
    } catch {
      /* not known here */
    }
  }
  try {
    ms_.setActionHandler('seekto', (d) => d.seekTime !== undefined && seek(d.seekTime));
  } catch {
    /* older browsers */
  }
}

/** The engine's audio element, for measuring (tools/gapless-lab, web/engine-lab.html); never for playback control. */
export const engineAudio = () => audio;

// --- played elsewhere: the app's own player, or another of Cratarr's players; what it plays, as it reports it ---

/** One of Cratarr's players the page drives (Play On), by name; null: this browser or the app plays. */
let driven: string | null = null;
/** Whether the queue plays elsewhere than in this page's stream. */
const handed = () => inApp || driven !== null;

/** From now on the queue plays on one of Cratarr's players (by name), or here again (`null`). What `local` holds is
 *  then that player's, as it reports it (`reported`). */
export function drive(name: string | null) {
  driven = name;
  // The next player's own, from its first report, on its own clock.
  deck = null;
  eased = null;
  clockGaps = [];
  landingCourse = null;
  landingEase = null;
  local.sleep = null;
  reported = { positionMs: 0, at: performance.now() };
}

/** Here again after driving another player, which plays on: what `local` held was that player's, so it is let go
 *  (no command to it, nothing kept in this browser's list). */
export function emptyHere() {
  local.queue = [];
  local.index = -1;
  local.position = 0;
  local.paused = true;
}

/** The last report: the position then (ms), and when it came; the position in between is counted on here. */
let reported = { positionMs: 0, at: 0 };
/** The app's deck as it last said (0.9.14-2): its place then (s), when it was read, its speed (track s per s,
 *  signed) and how fast that changes (per s). The place each moment follows from them, not from the reports' 250 ms steps
 *  (the owner, 2026-10-28: the waveform and the deck moved in strides through a wind); a report's correction fades in
 *  (lib/deckCourse; the owner, 2026-10-29: still strides). Null: an app from before, counted on as it was. */
let deck: Course | null = null;
/** A report's correction still fading in, and the two clocks' differences the deck's read time is taken from (one
 *  player's: begun anew with each). */
let eased: Ease | null = null;
let clockGaps: number[] = [];
/** Whether the app's deck goes backwards now (lib/SeekWave: a track change's colours from the right). */
export const deckNow = $state({ back: false });
/** The deck's place now, as shown: its course (never past a stand), a correction fading in, within the track. */
function deckPlace(d: Course, now = performance.now()): number {
  const p = shownAt(d, eased, now);
  return Math.max(0, local.duration ? Math.min(p, local.duration) : p);
}
/** The app's deck's speed now (× the track's, signed), from its last report; null: not said (here, or an older app). */
export function deckSpeed(now = performance.now()): number | null {
  const d = deck;
  if (!d || !handed()) return null;
  const v = d.v + d.a * Math.max(0, (now - d.at) / 1000);
  return d.v !== 0 && Math.sign(v) !== Math.sign(d.v) ? 0 : v;
}
let deckFrame = 0;
/** The deck moving other than playing on at its own speed, or a correction fading in: its place each frame (at 1× the
 *  counting on is enough). */
function deckMoves() {
  if (deckFrame) return;
  const step = () => {
    deckFrame = 0;
    const d = deck;
    const steady = !!d && ((d.v === 1 && d.a === 0) || (d.v === 0 && d.a === 0)) && !easing(eased, performance.now());
    if (!d || !handed() || steady) return;
    local.position = deckPlace(d);
    // The app's look-ahead goes with it, frame by frame.
    if (appHolding && landingCourse) holdNow.landing = landingPlace();
    deckFrame = requestAnimationFrame(step);
  };
  deckFrame = requestAnimationFrame(step);
}
let heldTimer: ReturnType<typeof setTimeout> | undefined;
const HELD_GRACE_MS = 600;
/** Let go: until the player says its wind has landed (a moment after), at most WIND_LAND_MS. */
const WIND_LAND_MS = 6000;
function holdThere(on: boolean) {
  clearTimeout(heldTimer);
  if (on) drivenHeld = drivenHold.press();
  else {
    drivenHeld = drivenHold.letGo(drivenHeld);
    heldTimer = setTimeout(() => (drivenHeld = drivenHold.over(drivenHeld)), WIND_LAND_MS);
  }
}
/** The player says it no longer winds: after a let-go hold, the hold is over a moment later. */
function heldLanded() {
  if (!drivenHold.landing(drivenHeld)) return;
  clearTimeout(heldTimer);
  heldTimer = setTimeout(() => (drivenHeld = drivenHold.over(drivenHeld)), HELD_GRACE_MS);
}
let glideFrame = 0;
function gliding() {
  if (glideFrame) return;
  const step = () => {
    glideFrame = 0;
    const g = tape.glide;
    if (!g || !handed()) return;
    const u = Math.min(1, (performance.now() - g.at) / g.span);
    local.position = g.from + (g.to - g.from) * u;
    glideFrame = requestAnimationFrame(step);
  };
  glideFrame = requestAnimationFrame(step);
}
/** A queue's track as itself (its entry too), or null. */
const trackKey = (t: LocalTrack | undefined) => (t ? `${t.source}/${t.id}/${t.entry ?? ''}` : null);
const sameQueue = (a: LocalTrack[], b: LocalTrack[]) =>
  a.length === b.length && a.every((t, i) => t.source === b[i].source && t.id === b[i].id && t.entry === b[i].entry);

/** The player that plays: what it plays now (`queue` only when it changed). */
export function reportedBy(m: {
  queue?: unknown;
  index?: unknown;
  positionMs?: unknown;
  durationMs?: unknown;
  paused?: unknown;
  error?: unknown;
  repeat?: unknown;
  sleep?: unknown;
  radio?: unknown;
  radioBy?: unknown;
  radioCount?: unknown;
  skipFavourites?: unknown;
  autoplay?: unknown;
  levelDb?: unknown;
  rewinding?: unknown;
  direction?: unknown;
  starting?: unknown;
  speedX?: unknown;
  accelX?: unknown;
  /** When the deck was read: ms of the app's elapsedRealtime (0.9.15-2); an app from before says nothing. */
  deckAtMs?: unknown;
  landing?: unknown;
}) {
  const trackBefore = trackKey(local.queue[local.index]);
  if (m.repeat === 'off' || m.repeat === 'one' || m.repeat === 'all') local.repeat = m.repeat;
  // Left out: not said (a report of the queue alone); null: none.
  if (m.sleep !== undefined) local.sleep = sleepOf(m.sleep);
  if (typeof m.radio === 'boolean') local.radio = m.radio;
  // An app from before the amendment reports its Autoplay as `radio`, an object.
  else if (m.radio && typeof m.radio === 'object') local.autoplay = autoplayOf(fromOld(m.radio as Record<string, unknown>));
  if (RADIO_BYS.includes(m.radioBy as RadioBy)) local.radioBy = m.radioBy as RadioBy;
  if (typeof m.radioCount === 'number' && m.radioCount > 0) local.radioCount = m.radioCount;
  if (typeof m.skipFavourites === 'boolean') local.skipFavourites = m.skipFavourites;
  if (m.autoplay !== undefined) local.autoplay = autoplayOf(m.autoplay);
  if (Array.isArray(m.queue) && !sameQueue(m.queue as LocalTrack[], local.queue)) {
    // A queue the player changed (moved, added to) keeps the names this page already knows of its tracks.
    const known = new Map(local.queue.filter((t) => t.name).map((t) => [`${t.source}/${t.id}`, t]));
    local.queue = (m.queue as LocalTrack[]).map((t) => (t.name ? t : (known.get(`${t.source}/${t.id}`) ?? t)));
    // A queue the page did not order itself (a page opened while it played): shuffled when its order says so.
    local.shuffled = looksShuffled(local.queue, typeof m.index === 'number' ? m.index : -1);
  }
  local.index = typeof m.index === 'number' ? m.index : -1;
  local.paused = m.paused !== false && !tape.heldThere;
  local.duration = typeof m.durationMs === 'number' && m.durationMs > 0 ? m.durationMs / 1000 : null;
  local.error = typeof m.error === 'string' ? m.error : null;
  // The app's own level (0.9.8): what it applies now; 0 when it levels nothing or says nothing.
  local.levelDb = typeof m.levelDb === 'number' && m.levelDb < 0 ? m.levelDb : 0;
  // Asked to play and not sounding yet (app, 2026-10-28); an app from before says nothing: not starting.
  if (m.starting !== undefined) local.starting = m.starting === true;
  const now = performance.now();
  const pos = (Number(m.positionMs) || 0) / 1000;
  if (typeof m.speedX === 'number' && Number.isFinite(m.speedX)) {
    // The place shown until now: the new course's correction fades in from it, on the same track (lib/deckCourse).
    const before = deck ? deckPlace(deck, now) : null;
    deck = { pos, at: readAt(clockGaps, m.deckAtMs, now), v: m.speedX, a: typeof m.accelX === 'number' && Number.isFinite(m.accelX) ? m.accelX : 0 };
    eased = easeInto(before, deck, now, trackKey(local.queue[local.index]) === trackBefore);
    deckNow.back = deck.v < 0;
    tape.glide = null;
    local.position = deckPlace(deck, now);
    deckMoves();
  } else if (m.rewinding === true) {
    deck = null;
    eased = null;
    tape.glide = { from: local.position, to: pos, at: now, span: Math.min(1000, Math.max(100, now - reported.at)) };
    gliding();
  } else {
    deck = null;
    eased = null;
    tape.glide = null;
    local.position = pos;
  }
  reported = { positionMs: pos * 1000, at: now };
  // The app's tape rewind: its position going back as it reports it, shown as it goes (lib/remote).
  if (typeof m.rewinding === 'boolean') {
    if (m.rewinding) windAsked(false);
    else {
      heldLanded();
      // Its wind ended: where it landed is a place of its own (lib/remote: no hold against the last place shown).
      if (rewindNow.on) sought.n++;
    }
    rewindNow.on = m.rewinding;
  }
  // Its direction knob, as it stands there.
  if (m.direction === 'L' || m.direction === 'R') tapeDir.want = m.direction;
  if (m.landing !== undefined) appLanding(m.landing);
}
/** The app's hold (0.9.14-4): where it lands, held or let go (null once there): the look-ahead, the hold's zoom
 *  and the aim's check, as for this page's own. Held while it winds faster than it plays; let go once it brakes. */
let appHolding = false;
/** The app's look-ahead between its reports: where it lands moves as its deck does, so it is carried along with the
 *  deck, each report's correction faded in (lib/deckCourse; the owner, 2026-10-29: it trembled against the playhead
 *  counted on smoothly, standing still between reports and then jumping). */
let landingCourse: Course | null = null;
let landingEase: Ease | null = null;
const landingPlace = (now = performance.now()) => (landingCourse ? Math.max(0, shownAt(landingCourse, landingEase, now)) : null);
function appLanding(l: unknown) {
  const at = l && typeof l === 'object' ? (l as Record<string, unknown>) : null;
  const ok = at && typeof at.index === 'number' && typeof at.positionMs === 'number';
  if (!ok) {
    if (!appHolding) return;
    appHolding = false;
    // There: the line as last painted against where the deck is.
    const t = local.queue[local.index];
    playback('info', 'aim vs resume', {
      way: holdNow.dir < 0 ? 'back: stand' : 'forward: 1x',
      app: true,
      aimed: aimPainted.pos === null ? null : Math.round(aimPainted.pos * 1000),
      resumed: Math.round(positionNow() * 1000),
      off_ms: aimPainted.pos === null || aimPainted.track !== (t ? `${t.source}/${t.id}` : null) ? null : Math.round((positionNow() - aimPainted.pos) * 1000),
    });
    Object.assign(holdNow, { on: false, held: false, landing: null, landingIndex: null, speed: 0 });
    landingCourse = null;
    landingEase = null;
    return;
  }
  const index = at.index as number;
  const pos = (at.positionMs as number) / 1000;
  const v = deck?.v ?? 0;
  // Read with the deck, in the same report: from when the deck was read, at its speed.
  const now = performance.now();
  const before = landingCourse && holdNow.landingIndex === index ? landingPlace(now) : null;
  landingCourse = { pos, at: deck?.at ?? now, v, a: deck?.a ?? 0 };
  landingEase = easeInto(before, landingCourse, now, true);
  appHolding = true;
  holdNow.on = true;
  holdNow.landing = landingPlace(now);
  holdNow.landingIndex = index;
  holdNow.speed = Math.abs(v);
  // The app's top speed, for the zoom's "at its top" (lib/SeekWave): its holds fly to FLIGHT_TOP, not this page's
  // HOLD_TOP (the owner, 2026-10-29: the zoom went back to the whole as the app passed 16×); its own word where it says.
  holdNow.top = typeof at.topX === 'number' && at.topX > 1 ? at.topX : FLIGHT_TOP;
  holdNow.dir = index > local.index || (index === local.index && pos > local.position) ? 1 : -1;
  // Held: still getting faster (or at its top); let go, it brakes.
  holdNow.held = (deck?.a ?? 0) >= 0 && Math.abs(v) > 1;
}

function sleepOf(s: unknown): Sleep | null {
  if (!s || typeof s !== 'object') return null;
  const { atMs, endOfTrack } = s as Record<string, unknown>;
  const timer = { atMs: typeof atMs === 'number' ? atMs : null, endOfTrack: endOfTrack === true };
  return timer.atMs === null && !timer.endOfTrack ? null : timer;
}

/** A reported Autoplay (the app's whole setting, or a player view's mode and switches); null while off. */
function autoplayOf(a: unknown): Autoplay | null {
  if (!a || typeof a !== 'object') return null;
  const { mode, albums, restart, seed, within } = a as Record<string, unknown>;
  return {
    mode: AUTOPLAY_MODES.includes(mode as AutoplayMode) ? (mode as AutoplayMode) : 'similar',
    albums: albums === true,
    restart: restart === true,
    seed: Array.isArray(seed) ? seed.filter((x): x is string => typeof x === 'string') : [],
    within: typeof within === 'string' && within ? within : null,
  };
}

/** A radio from before the amendment (`{on, mode, seed, within}`), as Autoplay. */
function fromOld(r: Record<string, unknown>): Record<string, unknown> | null {
  if (r.on === false) return null;
  const mode = r.mode === 'artist' || r.mode === 'era' || r.mode === 'random' ? r.mode : 'similar';
  return { ...r, mode, albums: r.mode === 'album_mix', restart: r.mode === 'reshuffle' };
}

/** The sleep timer, for the player that plays elsewhere (the app, Cratarr's player), which keeps it: it is sure to be
 *  awake when it fires. `null` clears it. Its reports set `local.sleep`. */
export function handSleep(s: Sleep | null) {
  if (!handed()) return;
  local.sleep = s;
  hand('sleep', { atMs: s?.atMs ?? null, endOfTrack: s?.endOfTrack ?? false });
}

/** A call for the player that plays: the app's own (its op, which every app takes), or the one driven (the server's
 *  PlayerCommand for the op, lib/player/commands). */
function hand(op: string, args: Record<string, unknown> = {}) {
  if (driven === null) return toApp({ type: 'player', op, ...args });
  const command = commandOf(op, args);
  if (!command) return;
  // A failed command is a toast of its own: `local.error` is the playing track's, and the player's next report clears it
  // (the architecture audit, 2026-10-29: the toast went within a second).
  playerCommand(driven, command).catch((e) => toast.fail(e, "The player didn't take it"));
}

/** The position now, to the frame (the visuals' beats, decision 0046), where `local.position` moves four times a
 *  second: counted on from the playing player's last report, or read from this page's own audio. */
export function positionNow(): number {
  if (!handed()) return tape.rewindPos ?? where()?.offset ?? local.position;
  if (deck) return deckPlace(deck);
  if (tape.glide) return local.position;
  const p = (reported.positionMs + sinceReport()) / 1000;
  return local.duration ? Math.min(p, local.duration) : p;
}
/** How far the app's player has played since its last report (ms). Not through its Start and stop ramps (the owner,
 *  2026-10-26: no jump as one ends): it reports the press's place meanwhile and plays on from there once it ends. */
function sinceReport(): number {
  // A rewind goes back: nothing counted on between its reports.
  if (local.paused || ramping.dir !== null || rewindNow.on) return 0;
  return performance.now() - Math.max(reported.at, ramping.ended);
}

if (inApp) onApp('player', (m) => driven === null && reportedBy(m as Parameters<typeof reportedBy>[0]));
if (typeof window !== 'undefined') {
  setInterval(() => {
    if (!handed() || local.index < 0) return;
    // The app's deck: where its speed has brought it (still, it stands).
    if (deck) return void (local.position = deckPlace(deck));
    if (local.paused || ramping.dir !== null || rewindNow.on) return;
    const p = (reported.positionMs + sinceReport()) / 1000;
    local.position = local.duration ? Math.min(p, local.duration) : p;
  }, 250);
}

// --- what the player and the lock screen ask of it ---

/** The tracks added by hand still to come: the run of them right after the playing track (decision 0033). */
export function addedAhead(queue: LocalTrack[], index: number): LocalTrack[] {
  const run: LocalTrack[] = [];
  for (let i = index + 1; i < queue.length && queue[i].lane === 'next'; i++) run.push(queue[i]);
  return run;
}

/** The album's or playlist's own order kept on its tracks (`n`), for turning shuffle off later. */
const numbered = (tracks: LocalTrack[]) => tracks.map((t, i) => (t.lane || t.n !== undefined ? t : { ...t, n: i }));

/** What comes of the album or playlist in its own order again, from the playing track on; those from before it last,
 *  so nothing is lost. Tracks without a place keep theirs, at the end. */
function inOrder(rest: LocalTrack[], from: number): LocalTrack[] {
  const placed = rest.filter((t) => t.n !== undefined).sort((a, b) => a.n! - b.n!);
  return [...placed.filter((t) => t.n! > from), ...placed.filter((t) => t.n! <= from), ...rest.filter((t) => t.n === undefined)];
}

/** The place in the album or playlist the queue has reached: the playing track's, else the last one's before it. */
function reached(queue: LocalTrack[], index: number): number {
  for (let i = index; i >= 0; i--) if (queue[i]?.n !== undefined) return queue[i].n!;
  return -1;
}

/** Whether what comes of the album or playlist is out of its own order: more than the one turn back that starting
 *  partway in gives. */
function looksShuffled(queue: LocalTrack[], index: number): boolean {
  const ns = queue.slice(index + 1).flatMap((t) => (t.lane || t.n === undefined ? [] : [t.n]));
  let back = 0;
  for (let i = 1; i < ns.length; i++) if (ns[i] < ns[i - 1]) back++;
  return back > 1;
}

/** Shuffle on (what comes of the album or playlist, mixed) or off (back in its own order, from the playing track on).
 *  What was added by hand stays ahead, in its order (decision 0033). */
export function setShuffle(on: boolean) {
  local.shuffled = on;
  if (local.index < 0) return;
  // A player of Cratarr's: Cratarr orders its queue (it holds it, decision 0033), not this page from what it last saw.
  if (driven !== null) return hand('shuffle', { on });
  const ahead = addedAhead(local.queue, local.index);
  const after = local.queue.slice(local.index + 1 + ahead.length);
  // Only the queue's own tracks are mixed (decision 0036, amended): Radio's inserts for the playing track stay right
  // after it, Autoplay's tail stays last, each in its order.
  let run = 0;
  while (run < after.length && after[run].lane === 'radio') run++;
  const rest = after.slice(run);
  const own = rest.filter((t) => !t.lane);
  const others = rest.filter((t) => t.lane && t.lane !== 'autoplay');
  const tail = rest.filter((t) => t.lane === 'autoplay');
  replaceUpcoming([...ahead, ...after.slice(0, run), ...(on ? shuffled(own) : inOrder(own, reached(local.queue, local.index))), ...others, ...tail]);
}

/** Shuffle, once (the owner, 2026-10-23: a queue always has an order; shuffle is an action, not a state): what comes of
 *  the queue's own tracks mixed, the playing track kept; what was added by hand stays ahead, Radio's inserts right after
 *  the playing track and Autoplay's tail last, as setShuffle does. Returns the order before, for its Undo
 *  (`restoreUpcoming`); null with nothing to mix. */
export function shuffleUpcoming(): LocalTrack[] | null {
  if (local.index < 0) return null;
  const before = local.queue.slice(local.index + 1);
  const ahead = addedAhead(local.queue, local.index);
  const after = before.slice(ahead.length);
  let run = 0;
  while (run < after.length && after[run].lane === 'radio') run++;
  const rest = after.slice(run);
  const own = rest.filter((t) => !t.lane);
  if (own.length < 2) return null;
  const others = rest.filter((t) => t.lane && t.lane !== 'autoplay');
  const tail = rest.filter((t) => t.lane === 'autoplay');
  local.shuffled = false;
  replaceUpcoming([...ahead, ...after.slice(0, run), ...shuffled(own), ...others, ...tail]);
  return before;
}

/** Repeat off, one or all: the player that plays applies it (here, the app, or Cratarr's player). Repeat all turns
 *  Autoplay off: both say what comes when the queue ends (decision 0036, amended). Radio lives with both. */
export function setRepeat(mode: Repeat) {
  local.repeat = mode;
  if (mode === 'all' && local.autoplay) {
    local.autoplay = null;
    dropAhead('autoplay');
  }
  if (handed()) return hand('repeat', { mode });
  nextRound();
}

/** Radio on or off (decision 0036, amended). While on, the player that plays inserts `count` tracks like each of the
 *  queue's own tracks (`by` the track, its artist or its era) right after it as it starts (this page for its own
 *  stream, else the app or Cratarr's player). Off, or another choice or count while on, takes the inserts still to come
 *  out; on, the playing track gets them anew. What is not given stays. */
export function setRadio(on: boolean, opts: { by?: RadioBy; count?: number } = {}) {
  // Switched on from off: as the defaults say, where the choice does not (the owner, 2026-10-25).
  if (on && !local.radio && defaultsKnown.yes) opts = { by: opts.by ?? defaults.radio.by, count: opts.count ?? defaults.radio.count };
  const count = opts.count === undefined ? undefined : Math.min(Math.max(Math.round(opts.count), 1), RADIO_MAX);
  const other = (opts.by !== undefined && opts.by !== local.radioBy) || (count !== undefined && count !== local.radioCount);
  const anew = local.radio && on && other;
  if (opts.by !== undefined) local.radioBy = opts.by;
  if (count !== undefined) local.radioCount = count;
  local.radio = on;
  if (!on || anew) dropAhead('radio');
  if (handed()) {
    return hand('radio', { on, ...(opts.by !== undefined ? { by: opts.by } : {}), ...(count !== undefined ? { count } : {}) });
  }
  if (anew) insertedFor.clear();
  if (on) insertRadio();
}

/** Radio's and Autoplay's picks leave the owner's favourites out, or not (decision 0036, amended). A change takes their
 *  picks still to come out, and the player that plays picks anew: Radio for the playing track, Autoplay when the queue
 *  runs low. */
export function setSkipFavourites(on: boolean) {
  if (on === local.skipFavourites) return;
  local.skipFavourites = on;
  dropAhead('radio');
  dropAhead('autoplay');
  if (handed()) return hand('skip_favourites', { on });
  insertedFor.clear();
  fill();
}

/** What Radio and Autoplay choose from changed (lib/mySettings' more_music, already kept): their picks still to come
 *  out, and picked anew from there, as when favourites are left out. */
export function sourcesChanged() {
  dropAhead('radio');
  dropAhead('autoplay');
  if (handed()) return;
  insertedFor.clear();
  fill();
}

/** Autoplay in a mode, or 'stop' (decision 0036, amended): the player that plays appends its tracks when the queue runs
 *  out. What is not given stays as it was; another mode, albums or crate lets its earlier picks make way. On turns
 *  repeat all off. */
export function setAutoplay(mode: AutoplayMode | 'stop', opts: { albums?: boolean; restart?: boolean; seed?: string[]; within?: string | null } = {}) {
  if (mode === 'stop') {
    local.autoplay = null;
    dropAhead('autoplay');
    if (handed()) hand('autoplay', { mode: 'stop' });
    return;
  }
  const was = local.autoplay;
  const next: Autoplay = {
    mode,
    // Switched on from off: whole albums as the defaults say (the owner, 2026-10-25).
    albums: opts.albums ?? was?.albums ?? (defaultsKnown.yes ? defaults.autoplay.albums : false),
    restart: opts.restart ?? was?.restart ?? false,
    seed: opts.seed ?? was?.seed ?? [],
    within: opts.within === undefined ? (was?.within ?? null) : opts.within,
  };
  if (was && (was.mode !== next.mode || was.albums !== next.albums || was.within !== next.within)) dropAhead('autoplay');
  local.autoplay = next;
  if (local.repeat === 'all') local.repeat = 'off';
  // What is given only: the player keeps the rest (a player's view does not show the seed here).
  if (handed()) {
    return hand('autoplay', {
      mode,
      ...(opts.albums !== undefined ? { albums: opts.albums } : {}),
      ...(opts.restart !== undefined ? { restart: opts.restart } : {}),
      ...(opts.seed ? { seed: opts.seed } : {}),
      ...(opts.within !== undefined ? { within: opts.within ?? '' } : {}),
    });
  }
  fillAutoplay();
}

/** Auto arm (lib/mySettings' defaults, the owner 2026-10-25), as a new queue starts: armed, Autoplay that was off
 *  starts as the defaults say. Never off by it: on, Autoplay stays as it is, armed or not ("I don't want autoplay to
 *  stop in that case"). */
function arm() {
  if (!defaultsKnown.yes) return;
  const a = defaults.autoplay;
  if (a.arm && !local.autoplay) local.autoplay = { mode: a.mode, albums: a.albums, restart: false, seed: [], within: null };
  if (local.autoplay && local.repeat === 'all') local.repeat = 'off';
}
/** Auto arm told to the player that plays elsewhere, after its new queue. */
function handArm() {
  if (!defaultsKnown.yes) return;
  hand('autoplay', local.autoplay ? { mode: local.autoplay.mode, albums: local.autoplay.albums } : { mode: 'stop' });
}

/** One's own choices (decision 0051, lib/mySettings), for a page that starts with nothing playing: Radio as it starts,
 *  leaving favourites out, and auto arm. A player's own state, reported, comes over them. */
export function startWith(_c: Choices) {
  if (local.queue.length || local.index >= 0 || !defaultsKnown.yes) return;
  if (RADIO_BYS.includes(defaults.radio.by)) local.radioBy = defaults.radio.by;
  local.radioCount = Math.min(Math.max(Math.round(defaults.radio.count), 1), RADIO_MAX);
  local.skipFavourites = defaults.skipFavourites;
  arm();
}

/** A lane's tracks still to come, out here (the player that plays elsewhere does the same with what it is told). */
function dropAhead(lane: 'radio' | 'autoplay') {
  if (local.index < 0) return;
  const kept = local.queue.slice(local.index + 1).filter((t) => t.lane !== lane);
  if (kept.length === local.queue.length - local.index - 1) return;
  if (handed()) local.queue = [...local.queue.slice(0, local.index + 1), ...kept];
  else replaceUpcoming(kept);
}

/** "Start radio" from an item (decision 0036; amended: it sets Autoplay): tracks like `seed` (its own left out) play,
 *  and Autoplay goes on with that seed. `within`: a crate's id, to play only the crate's tracks ("Play as radio"; its
 *  own then may play). `from`: its name, for the queue sheet. */
export async function startRadio(seed: string[], from: string | null, within: string | null = null) {
  const mode = local.autoplay?.mode ?? 'similar';
  const albums = local.autoplay?.albums ?? false;
  const skip_favourites = local.skipFavourites;
  const { tracks } = await radioNext({ mode, albums, seed, exclude: within ? [] : seed, last: null, count: BATCH, within, skip_favourites });
  if (!tracks.length) throw new Error('Nothing like it was found.');
  // Set before the play: a play of Autoplay's tracks only keeps its seed.
  local.autoplay = { mode, albums, restart: false, seed, within };
  if (local.repeat === 'all') local.repeat = 'off';
  if (handed()) hand('autoplay', { mode, albums, restart: false, seed, within: within ?? '' });
  playQueue(tracks as LocalTrack[], 0, from);
}

/** How many tracks are asked for at a time, and how few may be left ahead before Autoplay asks for more. */
const BATCH = 5;
const LOW = 3;
let filling = false;
/** The own tracks Radio has asked inserts for (by entry, or place and id): one ask per track. */
const insertedFor = new Set<string>();

/** This page's own stream: Radio's inserts for the playing own track, and Autoplay's tracks when the queue runs low. */
function fill() {
  insertRadio();
  fillAutoplay();
}

/** Radio, this page's own stream: about five tracks like the own track that plays, right after it and its hand-added
 *  run, unless they are there already. */
function insertRadio() {
  if (!local.radio || handed() || local.index < 0) return;
  const cur = local.queue[local.index];
  if (!cur || cur.lane) return;
  const key = cur.entry ?? `${local.index}:${cur.id}`;
  const at = local.index + 1 + addedAhead(local.queue, local.index).length;
  if (insertedFor.has(key) || local.queue[at]?.lane === 'radio') return;
  insertedFor.add(key);
  const { radioBy: by, radioCount: count, skipFavourites: skip_favourites } = local;
  radioNext({ mode: 'radio', by, seed: [cur.id], exclude: local.queue.map((t) => t.id), last: cur.id, count, within: null, skip_favourites })
    .then(({ tracks }) => {
      // Still on as asked, still that track, nothing inserted meanwhile: right after it and its hand-added run.
      if (!local.radio || local.radioBy !== by || local.radioCount !== count || local.skipFavourites !== skip_favourites) return;
      if (handed() || !tracks.length || local.queue[local.index] !== cur) return;
      const here = local.index + 1 + addedAhead(local.queue, local.index).length;
      if (local.queue[here]?.lane === 'radio') return;
      local.queue = [...local.queue.slice(0, here), ...(tracks as LocalTrack[]), ...local.queue.slice(here)];
      cutFrom(here);
    })
    .catch((e) => {
      insertedFor.delete(key);
      local.error = e instanceof Error ? e.message : String(e);
    });
}

/** Autoplay, this page's own stream: when fewer than three tracks are left ahead, Autoplay's picks, appended after
 *  everything. (`restart`, the queue's own tracks again, is no more: the owner, 2026-10-24; one still set does
 *  nothing.) */
function fillAutoplay() {
  const auto = local.autoplay;
  if (!auto || handed() || filling || local.index < 0 || local.queue.length - local.index - 1 >= LOW) return;
  const ask = {
    mode: auto.mode,
    albums: auto.albums,
    seed: auto.seed.length ? auto.seed : local.queue.filter((t) => !t.lane).map((t) => t.id),
    exclude: local.queue.map((t) => t.id),
    last: local.queue[local.index]?.id ?? null,
    count: BATCH,
    within: auto.within,
    skip_favourites: local.skipFavourites,
  };
  filling = true;
  radioNext(ask)
    .then(({ tracks }) => {
      // Still on as asked, still here: appended after everything, so the stream goes on into them.
      if (!local.autoplay || handed() || !tracks.length || local.skipFavourites !== ask.skip_favourites) return;
      local.queue = [...local.queue, ...(tracks as LocalTrack[])];
      if (tape.nextWhenFilled) {
        nextKept = pendingNext.filled(nextKept);
        next();
      }
    })
    .catch((e) => (local.error = e instanceof Error ? e.message : String(e)))
    .finally(() => {
      filling = false;
      nextKept = pendingNext.filled(nextKept);
    });
}

/** Under repeat all, while the last track plays here: the next round of the album or playlist after it (decision 0033,
 *  amended: those added by hand play once; shuffled anew when the queue plays shuffled, not starting with the track
 *  that just played). The stream then goes on into it without a gap. */
function nextRound() {
  if (local.repeat !== 'all' || local.index < 0 || local.index !== local.queue.length - 1) return;
  const byPlace = new Map<number, LocalTrack>();
  for (const t of local.queue) if (!t.lane && t.n !== undefined) byPlace.set(t.n, t);
  let round = byPlace.size
    ? [...byPlace.entries()].sort((a, b) => a[0] - b[0]).map(([, t]) => t)
    : local.queue.filter((t, i, all) => !t.lane && all.findIndex((x) => x.source === t.source && x.id === t.id) === i);
  if (!round.length) return;
  if (local.shuffled) {
    round = shuffled(round);
    const last = local.queue[local.index];
    if (round.length > 1 && round[0].source === last.source && round[0].id === last.id) round.push(round.shift()!);
  }
  // New entries for the new round's tracks; their places stay.
  local.queue = [...local.queue, ...round.map(({ entry: _, ...t }) => t)];
}

/** Everything after the playing track, replaced; the playing track plays on. */
function replaceUpcoming(upcoming: LocalTrack[]) {
  local.queue = [...local.queue.slice(0, local.index + 1), ...upcoming];
  if (handed()) return hand('upcoming', { tracks: upcoming, shuffled: local.shuffled });
  cutFrom(local.index + 1);
}

/** Takes out everything after the playing track (the queue sheet's clear button); returns it, for its Undo. Autoplay,
 *  when on, fills again at the next track. */
export function clearUpcoming(): LocalTrack[] {
  const removed = local.queue.slice(local.index + 1);
  if (removed.length) replaceUpcoming([]);
  return removed;
}

/** Puts back what `clearUpcoming` took out (its Undo), in place of what came meanwhile (Autoplay's). */
export const restoreUpcoming = (tracks: LocalTrack[]) => replaceUpcoming(tracks);

/** Plays `tracks` from `index`, `position` seconds into it; `shuffle`: all of them mixed, from the first. Something new
 *  replaces the album or playlist that played, not what was added by hand: those come right after its first track, as
 *  before (decision 0033's two lanes). */
export function playQueue(tracks: LocalTrack[], index = 0, from: string | null = null, position = 0, shuffle = false, moved = false) {
  // Played here: the queue it leaves is kept as it stood (decision 0033, amended); a player that plays elsewhere keeps
  // its own.
  const left = handed() ? null : standing();
  tracks = numbered(tracks);
  if (shuffle) [tracks, index, position] = [shuffled(tracks), 0, 0];
  // A queue moved here whole keeps its shuffle.
  local.shuffled = shuffle || looksShuffled(tracks, index);
  // Something new turns repeat one off; repeat all stays (decision 0033, amended).
  if (local.repeat === 'one') local.repeat = 'off';
  // Radio stays as it is; so does Autoplay, but off and armed it starts (the owner, 2026-10-25: auto arm), after the
  // new album or playlist, which is its seed. Autoplay's tracks only are "Start radio", which brings
  // its own seed (decision 0036, amended); a queue moved here whole keeps what it had.
  const anew = !moved && !tracks.every((t) => t.lane === 'autoplay');
  if (anew) {
    arm();
    if (local.autoplay) local.autoplay = { ...local.autoplay, seed: [], within: null };
  }
  // Those the new queue holds already (a queue moved here whole) are not added twice.
  const holds = (t: LocalTrack) => tracks.some((x) => x === t || (!!t.entry && x.entry === t.entry));
  const ahead = local.index < 0 ? [] : addedAhead(local.queue, local.index).filter((t) => !holds(t));
  if (ahead.length) tracks = [...tracks.slice(0, index + 1), ...ahead, ...tracks.slice(index + 1)];
  local.queue = tracks;
  local.from = from;
  if (handed()) {
    Object.assign(local, { index, position, paused: false, error: null });
    reported = { positionMs: position * 1000, at: performance.now() };
    hand('queue', { tracks, index, positionMs: Math.round(position * 1000), play: true, shuffled: local.shuffled, name: from });
    if (anew) handArm();
    return;
  }
  tapeForwards();
  atOnce();
  resetTape();
  keepLocal(left, newQueueId());
  fresh(index, position);
  fill();
}

/** Adds tracks by hand (decision 0033's two lanes): right after the playing one (`next`, so the newest come first), or
 *  after those added before (`last`), ahead of the rest of the album or playlist either way. */
export function enqueue(tracks: LocalTrack[], how: 'next' | 'last') {
  cue(how);
  tracks = tracks.map((t) => ({ ...t, lane: 'next' as const }));
  if (local.index < 0) return playQueue(tracks);
  const at = local.index + 1 + (how === 'next' ? 0 : addedAhead(local.queue, local.index).length);
  const after = at === 0 ? '' : local.queue[at - 1].entry;
  local.queue = [...local.queue.slice(0, at), ...tracks, ...local.queue.slice(at)];
  if (handed()) return hand('add', { at, tracks, ...(after === undefined ? {} : { after }) });
  cutFrom(at);
}

/** Moves upcoming track `from` to place `to` (queue indexes, both after the playing track): the queue sheet's drag. */
export function move(from: number, to: number) {
  const n = local.queue.length;
  if (from === to || from <= local.index || to <= local.index || from >= n || to >= n) return;
  const q = [...local.queue];
  const [t] = q.splice(from, 1);
  q.splice(to, 0, t);
  local.queue = q;
  // By entry where the tracks have them (decision 0033): the player's queue may have moved on since.
  const after = to === 0 ? '' : q[to - 1].entry;
  if (handed()) return hand('move', { from, to, ...(t.entry && after !== undefined ? { entry: t.entry, after } : {}) });
  cutFrom(Math.min(from, to));
}

/** Takes upcoming track `index` (a queue index after the playing track) out of the queue: the queue sheet's swipe. */
export function remove(index: number) {
  if (index <= local.index || index >= local.queue.length) return;
  const entry = local.queue[index].entry;
  local.queue = local.queue.filter((_, i) => i !== index);
  if (handed()) return hand('remove', { index, ...(entry ? { entry } : {}) });
  cutFrom(index);
}

/** As `remove`, counted in what comes next. */
export const removeUpcoming = (i: number) => remove(local.index + 1 + i);

/** Takes the playing track out of the queue: the next plays in its place from its start; after the last, the queue
 *  ends there, paused. The app's player is told `remove` with the playing entry, which it treats the same way. */
export function removePlaying() {
  const i = local.index;
  if (i < 0 || i >= local.queue.length) return;
  const entry = local.queue[i].entry;
  local.queue = local.queue.filter((_, j) => j !== i);
  const last = i >= local.queue.length;
  if (handed()) {
    Object.assign(local, { position: 0, paused: last, index: last ? local.queue.length - 1 : i });
    return hand('remove', { index: i, ...(entry ? { entry } : {}) });
  }
  if (last) {
    local.index = local.queue.length - 1;
    audio?.pause();
    return;
  }
  fresh(i, 0, true);
}

/** Takes the tracks `match` picks out of what plays and comes next (excluded, decision 0024 amended 2026-10-19): the
 *  playing one gives way to the next; what was played stays. */
export function dropFromQueue(match: (t: LocalTrack) => boolean) {
  for (let i = local.queue.length - 1; i > local.index; i--) if (match(local.queue[i])) remove(i);
  const now = local.queue[local.index];
  if (now && match(now)) removePlaying();
}

/** As `move`, counted in what comes next (0 = the track after the playing one), as the queue sheet lists it. */
export const moveUpcoming = (from: number, to: number) => move(local.index + 1 + from, local.index + 1 + to);

/** The queue changed from index `at` on: what the stream holds (or is fetching) from there is no longer what comes
 *  next. It is dropped, and the tick appends the new next track gaplessly after the last one kept. */
function cutFrom(at: number) {
  if (appending && appendingIndex >= at) {
    fetching?.abort();
    appending = false;
  }
  if (at > lastAppended) return;
  const keep = segments.filter((s) => s.index < at);
  // An ended stream counts too (the queue's last track was in): the removal below reopens it.
  if (!keep.length || !sb || !ms || ms.readyState === 'closed') return;
  const last = keep[keep.length - 1];
  const end = last.start + (last.length ?? 0);
  segments = keep;
  streamEnd = end;
  lastAppended = at - 1;
  (async () => {
    // An append that was under way finishes first; the aborted one adds nothing after it.
    while (sb?.updating) await waitUpdate();
    try {
      sb?.remove(end, Infinity);
    } catch {
      /* nothing appended there */
    }
  })();
}

/** Plays or pauses; `plain`: with no Start and stop (a switch between devices; the owner, 2026-10-27). */
export function togglePlay(plain = false) {
  if (handed()) {
    local.paused = !local.paused;
    return hand('toggle', plain ? { plain: true } : {});
  }
  if (!audio) return;
  // The direction knob at L, or playing backwards: its way.
  if (tapePress()) return;
  // During a wind it shows playing: a toggle pauses it where the counter is (the owner, 2026-10-27).
  if (tape.rewinding) return windTakeover('pause');
  if (plain) {
    atOnce();
    return void (audio.paused ? audio.play().catch(() => {}) : audio.pause());
  }
  // A ramp under way turns round (the audio still sounds while it winds down).
  if (tape.committed === true) return turnUp();
  if (tape.committed === false && ramping.dir === 'in') return void pauseRamped();
  if (audio.paused) rampIn();
  else void pauseRamped();
}

/** To queue track `index`: within the stream when it is already appended (gapless), else a fresh stream. */
export function jump(index: number) {
  if (handed() && index >= 0 && index < local.queue.length) {
    const entry = local.queue[index].entry;
    Object.assign(local, { index, position: 0, paused: false });
    return hand('jump', { index, ...(entry ? { entry } : {}) });
  }
  if (!audio || index < 0 || index >= local.queue.length) return;
  const was = lengthOf(local.index) ?? local.duration;
  if (local.index >= 0 && local.index !== index) local.left = { index: local.index, share: was ? Math.min(1, Math.max(0, local.position / was)) : 0 };
  // On from playing (a fast-forward's landing, a skip): the moment its stream takes is the tape's own, shown playing,
  // never as a stop (the owner, 2026-10-28: the next track's waveform came in grey).
  const onFromPlaying = !local.paused || !!tape.rewinding;
  tapeForwards();
  endRewind();
  atOnce();
  resetTape();
  if (onFromPlaying) {
    tape.quietPause = true;
    tape.quietUntil = performance.now() + PREP_WAIT_MS;
  }
  const seg = segments.find((s) => s.index === index);
  if (seg && audio.buffered.length && seg.start >= audio.buffered.start(0)) {
    audio.currentTime = seg.start;
    local.index = index;
    if (audio.paused) audio.play().catch(refused);
  } else if (comingFor(index, 0)) {
    // Readied already (a fast-forward's landing, lib's handover): played once it is in, never fetched again.
    Object.assign(local, { index, position: 0 });
    whenThere(index, 0, () => audio && audio.paused && void audio.play().catch(refused));
  } else {
    fresh(index, 0, true);
  }
  // Autoplay and Radio as after any change of track (the owner's tablet, 2026-10-27: skipping drained the queue).
  fill();
}

/** Next: the track wound on to its end, as Previous winds it back to its start (the owner, 2026-10-27: skip the
 *  mirror of rewind), then the next from its start; plainly where the device does not wind, or nothing can be. */
export function next() {
  // The app's player or one driven: its own Next (its wind-down, its fast-forward), never a jump to a track, which it
  // takes plainly (2026-10-27: the tablet's Next had no wind-down).
  if (handed()) return hand('next');
  // A wind-down is never cut short: the tape comes to its standstill first (the owner, 2026-10-27). A skip's own takes no
  // second Next (one tap, one skip); a pause's keeps it, and the next track starts up once the tape stands still.
  if (tape.committed === false && ramping.dir === 'out' && !tape.rewinding) return;
  if (tape.committed === true && ramping.dir === 'out' && !tape.rewinding) return void (nextKept = pendingNext.afterPause(nextKept));
  if (tape.rewinding) return windTakeover('next');
  // The last track, Autoplay on: on into its picks once they are in (asked now if not yet).
  if (local.index >= local.queue.length - 1 && local.autoplay) {
    nextKept = pendingNext.whenFilled(nextKept);
    return fill();
  }
  if (!handed() && windsOn('toEnd') && audio && !audio.paused && tape.committed === null) {
    const end = lengthOf(local.index);
    const at = where()?.offset ?? 0;
    if (end && end - at > 2 && (flyTo(end, 'toEnd') || startWind(end, 1, null, false, false, 'toEnd'))) return;
  }
  void skipRamped(() => jump(local.index + 1));
}

export function previous() {
  if (handed()) return hand('previous');
  // A second Previous while the first one's rewind runs: the track before (the owner, 2026-10-27: a double tap), its
  // rewind braking first where Start and stop is asked for next and previous too, else at once.
  const w = tape.rewinding;
  if (w && !w.reverse && w.to === 0 && w.dir < 0) {
    const before = () => {
      tape.quietPause = false;
      jump(Math.max(0, local.index - 1));
    };
    const s = prefs.startStop;
    if (w.at === null || s.mode === 'off' || !s.skips) {
      endRewind();
      return before();
    }
    return void settle().then(before);
  }
  if (tape.rewinding) return windTakeover('previous');
  // Repeat all: previous on the first track goes to the last.
  if (local.repeat === 'all' && local.index === 0 && local.position <= RESTART_AFTER && local.queue.length > 1) return void skipRamped(() => jump(local.queue.length - 1));
  if (local.position > RESTART_AFTER) {
    // Its start again: a tape's rewind where the device asks it.
    // Off: there plainly (never a seek's wind instead).
    if (!(windsOn('toStart') && startWind(0, -1, null, false, false, 'toStart'))) seek(0, false);
  } else void skipRamped(() => jump(Math.max(0, local.index - 1)));
}

/** Seeks asked of this device's player, counted (lib/remote: a paused position rests where it was pressed until one). */
export const sought = $state({ n: 0 });
/** To `secs` into the playing track: within what is loaded, else a fresh stream from there. */
/** `wind`: wound there whatever this device's own setting (another device asked it to, over Play On). */
export function seek(secs: number, wind?: boolean) {
  sought.n++;
  // A seek back or forward, each its own motion (the owner, 2026-10-28).
  const motion: PointMotion = secs < local.position ? 'seekBack' : 'seekFwd';
  const winds = wind ?? windsOn(motion);
  if (handed()) {
    // Winding there where this device winds seeks (the owner, 2026-10-27: over Play On too): the player is asked to.
    const wound = winds && !local.paused && Math.abs(secs - local.position) > 2;
    if (wound) {
      // It winds there: the counter goes with its reports, not to the place.
      windAsked(true);
      return hand('seek', { positionMs: Math.round(secs * 1000), wind: true });
    }
    local.position = secs;
    reported = { positionMs: secs * 1000, at: performance.now() };
    return hand('seek', { positionMs: Math.round(secs * 1000) });
  }
  if (!audio) return;
  // Played backwards (the knob at L): backwards on from the place sought, the knob as it is.
  if (tape.rewinding?.reverse && tape.rewinding.startAt === null) {
    const index = tape.rewinding.shown;
    endRewind();
    tape.quietPause = true;
    restAt(index, secs);
    if (!backwards()) {
      tape.quietPause = false;
      Object.assign(tapeDir, { want: 'R', state: 'forward' });
      void audio.play().catch(refused);
    }
    return;
  }
  // During a tape's rewind: it ends, and the track plays from there.
  if (tape.rewinding) {
    endRewind();
    // Wound into another track of the queue: that one, from there.
    if (where()?.seg.index !== local.index) return void fresh(local.index, secs, true);
    jumpTo(secs);
    if (audio.paused) void audio.play().catch(refused);
    return;
  }
  // During a Start and stop ramp the seek wins at once (the owner, 2026-10-26), with no gap added before it.
  if (tape.committed !== null) return void overrideRamp(secs);
  // Seeking winds the tape there where the device asks it (the owner, 2026-10-27): playing, more than a moment away.
  const here = where()?.offset ?? secs;
  const way: PointMotion = secs < here ? 'seekBack' : 'seekFwd';
  if (winds && !audio.paused && Math.abs(secs - here) > 2 && ((secs > here && flyTo(secs, way)) || startWind(secs, 1, null, false, false, way))) return;
  jumpTo(secs);
}
function jumpTo(secs: number) {
  if (!audio) return;
  const w = where();
  const target = w ? w.seg.start + secs : secs;
  for (let i = 0; i < audio.buffered.length; i++) {
    if (target >= audio.buffered.start(i) && target <= audio.buffered.end(i)) {
      audio.currentTime = target;
      return;
    }
  }
  fresh(local.index, secs, !audio.paused);
}

/** Stops and forgets the queue (switching output to elsewhere). Returns where it was, for the handover. */
export function stop(): { queue: LocalTrack[]; index: number; position: number } {
  const was = { queue: local.queue, index: local.index, position: local.position };
  if (handed()) hand('stop');
  else keepLocal(standing(), '');
  generation++;
  endRewind();
  atOnce();
  resetTape();
  fetching?.abort();
  audio?.pause();
  if (audio) audio.removeAttribute('src');
  local.queue = [];
  local.index = -1;
  local.paused = true;
  return was;
}

// --- Queue history in this browser (decision 0033, amended: a browser playing itself keeps its own, here only) ---

/** A queue left behind in this browser, whole, as it stood: its tracks, where it was, whether it played, its settings. */
export type KeptQueue = {
  id: string;
  name: string;
  renamed: boolean;
  /** When it was left (epoch s). */
  updated: number;
  tracks: LocalTrack[];
  index: number;
  position: number;
  playing: boolean;
  shuffled: boolean;
  repeat: Repeat;
  radio: boolean;
  radioBy: RadioBy;
  radioCount: number;
  skipFavourites: boolean;
  autoplay: Autoplay | null;
};

const KEPT_KEY = 'cratarr-queues';
/** Queues in all, with the one that plays. */
const QUEUES = 20;

function loadKept(): KeptQueue[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEPT_KEY) ?? '[]');
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

/** This browser's queues left behind, newest first; `id`, `renamed`: the one that plays. */
export const keptHere = $state({ list: loadKept(), id: '', renamed: false });

function saveKept() {
  try {
    localStorage.setItem(KEPT_KEY, JSON.stringify(keptHere.list));
  } catch {
    // A private window, or storage refused: the list lasts as long as the page.
  }
}

const newQueueId = () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`);

/** A queue's name when none was given: its album's, else its artist's, else its first track's. */
function nameOf(tracks: LocalTrack[]): string {
  const own = tracks.some((t) => !t.lane) ? tracks.filter((t) => !t.lane) : tracks;
  const same = (f: (t: LocalTrack) => string | null) => (own.every((t) => f(t) === f(own[0])) ? f(own[0]) : null);
  return (own.length && (same((t) => t.album) ?? same((t) => t.artist) ?? own[0].name)) || '';
}

/** The queue that plays here, as it stands, to keep as it is left behind; null when there is none. */
function standing(): KeptQueue | null {
  if (!local.queue.length) return null;
  return {
    id: keptHere.id || newQueueId(),
    name: local.from ?? nameOf(local.queue),
    renamed: keptHere.renamed,
    updated: Math.floor(Date.now() / 1000),
    tracks: local.queue,
    index: Math.max(0, local.index),
    position: local.position,
    playing: !local.paused,
    shuffled: local.shuffled,
    repeat: local.repeat,
    radio: local.radio,
    radioBy: local.radioBy,
    radioCount: local.radioCount,
    skipFavourites: local.skipFavourites,
    autoplay: local.autoplay,
  };
}

/** `left` kept, first (in place of an earlier copy of it); the queue that plays now is `id` (none: nothing plays). At
 *  most QUEUES in all: the oldest not renamed goes first. */
function keepLocal(left: KeptQueue | null, id: string) {
  let list = keptHere.list.filter((k) => k.id !== id && k.id !== left?.id);
  if (left) list = [left, ...list];
  while (list.length >= QUEUES) {
    const oldest = list.map((k) => !k.renamed).lastIndexOf(true);
    if (oldest < 0) break;
    list = list.filter((_, i) => i !== oldest);
  }
  keptHere.list = list;
  keptHere.id = id;
  keptHere.renamed = false;
  saveKept();
}

/** This browser's list as the queue sheet shows it: the queue that plays (marked), then those left, newest first. */
export function localQueues(): QueueItem[] {
  const now = local.queue.length
    ? [{ id: keptHere.id, name: local.from ?? nameOf(local.queue), tracks: local.queue.length, from: null, updated: Math.floor(Date.now() / 1000), playing: true, played_on: [] }]
    : [];
  return [...now, ...keptHere.list.map((k) => ({ id: k.id, name: k.name, tracks: k.tracks.length, from: null, updated: k.updated, playing: false, played_on: [] }))];
}

/** Takes up queue `id` of this browser's where it was left, playing if it played; the one it leaves is kept. */
export function switchLocal(id: string) {
  const k = keptHere.list.find((x) => x.id === id);
  if (!k || id === keptHere.id) return;
  keepLocal(standing(), k.id);
  keptHere.renamed = k.renamed;
  saveKept();
  Object.assign(local, {
    queue: k.tracks,
    shuffled: k.shuffled,
    repeat: k.repeat,
    radio: k.radio,
    radioBy: k.radioBy,
    radioCount: k.radioCount,
    skipFavourites: k.skipFavourites,
    autoplay: k.autoplay,
    from: k.name,
  });
  fresh(Math.min(k.index, k.tracks.length - 1), k.position, k.playing);
  fill();
}

/** Renames queue `id` of this browser's (the one that plays too); a renamed one stays until deleted. */
export function renameLocal(id: string, name: string) {
  name = name.trim().slice(0, 100);
  if (!name) return;
  if (id === keptHere.id && local.queue.length) {
    local.from = name;
    keptHere.renamed = true;
  } else {
    keptHere.list = keptHere.list.map((k) => (k.id === id ? { ...k, name, renamed: true } : k));
  }
  saveKept();
}

/** Takes queue `id` out of this browser's list; not the one that plays. */
export function deleteLocal(id: string) {
  if (id === keptHere.id && local.queue.length) return;
  keptHere.list = keptHere.list.filter((k) => k.id !== id);
  saveKept();
}

// --- Play On to this page (decision 0033: a page is a Play On target while it is open; 0031 amended) ---

/** Its volume as Cratarr sets it (0 to 100), and whether it is ducked (an announcement): the element's, times a duck. */
export const pageVolume = $state({ level: 100, ducked: false });
const DUCKED = 0.25;
function applyVolume() {
  if (audio) audio.volume = (pageVolume.level / 100) * (pageVolume.ducked ? DUCKED : 1) * rampGain;
  clipVolume();
}

// --- Start and stop (Settings › This device; the owner, 2026-10-25) ---
// Vinyl: the speed ramps, the pitch following, as a turntable spins up and down; Fade: the volume. Over the device's
// ramp, eased; a new one cuts one under way short. Natural track ends stay gapless (only play, pause and, where asked,
// next and previous ramp). Stepped by time, not by frames, so a hidden page still gets there.
let rampGain = 1;
let rampToken = 0;
/** Runs `go` quietly (a ramp reads it as it begins). */
function quietly<T>(go: () => T): T {
  own = ownRamp(true, own.turn);
  try {
    return go();
  } finally {
    queueMicrotask(() => (own = ownRamp(false, own.turn)));
  }
}
/** Start and stop as a ramp now uses it: a turn's own, else the device's. */
const ss = (): StartStop => (tape.turnRamp ? { ...prefs.startStop, ...tape.turnRamp } : prefs.startStop);
/** Each ramp's run; a seek cutting one short (overrideRamp) moves it on, and that ramp's own ending then does nothing. */
let run = 0;
/** A ramp's slowest: a little above the slowest the browser plays with sound (lib/rates). */
const RAMP_LOW = Math.max(0.07, RATE_FLOOR);
const easeInOut = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);
// A vinyl's speed: slow to leave full speed and quick at the still end winding down, the reverse winding up. Eased in
// and out, it was a third of full speed two thirds of the way and sounded stopped there (the owner, 2026-10-26).
const easeIn = (t: number) => t * t;
const easeOut = (t: number) => 1 - (1 - t) ** 2;
/** The deck's brake, the same for a pause and a wind (the owner, 2026-10-27): its speed on a raised cosine. */
const brakeCurve = (t: number) => (1 - Math.cos(Math.PI * t)) / 2;
/** The curves a Start and stop ramp goes by, up and down (a vinyl's speed, a fade's level): the ramps and the
 *  predictions of where they bring the tape read the same ones. */
const UP = { vinyl: easeOut, fade: easeInOut };
const DOWN = { vinyl: brakeCurve, fade: easeInOut };
const means = new Map<(t: number) => number, number>();
/** A curve's mean over its way (0..1), measured once from the curve itself. */
function meanOf(curve: (t: number) => number): number {
  let m = means.get(curve);
  if (m === undefined) {
    const N = 512;
    m = 0;
    for (let i = 0; i < N; i++) m += curve((i + 0.5) / N) / N;
    means.set(curve, m);
  }
  return m;
}
/** The track time a speed ramp on `curve` from `from`x to `to`x covers over `ms` (s). Every prediction of where the
 *  tape will be is this (a wind-up's early start before the press, where a stop will stand), so it holds whatever the
 *  curve (the owner, 2026-10-28: one curve prediction for all). */
const rampSpan = (curve: (t: number) => number, from: number, to: number, ms: number) => (ms / 1000) * (from + (to - from) * meanOf(curve));
/** The track time a Start and stop ramp covers over `ms` (s): a fade at full speed throughout; a vinyl's speed on its
 *  own curve, up from its slowest or down to it. */
function spanOf(mode: 'off' | 'vinyl' | 'fade', way: 'in' | 'out', ms: number, low = RAMP_LOW): number {
  if (mode === 'off') return 0;
  if (mode === 'fade') return ms / 1000;
  return way === 'in' ? rampSpan(UP.vinyl, low, 1, ms) : rampSpan(DOWN.vinyl, 1, low, ms);
}
function ramp(from: number, to: number, ms: number, set: (v: number) => void, ease = easeInOut): Promise<boolean> {
  const token = ++rampToken;
  const t0 = performance.now();
  return new Promise((done) => {
    const step = () => {
      if (token !== rampToken) return done(false);
      const t = Math.min(1, (performance.now() - t0) / ms);
      set(from + (to - from) * ease(t));
      if (t < 1) setTimeout(step, 16);
      else done(true);
    };
    step();
  });
}
/** Back to plain playing: full speed, its own pitch, full volume. */
function unramp() {
  rampToken++;
  rampGain = 1;
  if (audio) {
    audio.playbackRate = 1;
    audio.preservesPitch = true;
  }
  applyVolume();
}
/** A fade's loudness as heard: even in decibels (−60 dB at its quiet end, silence at 0), not in amplitude, which
 *  sounds loud and then drops at the end (the owner, 2026-10-26, of the app's first try). */
const heard = (v: number) => (v <= 0 ? 0 : v >= 1 ? 1 : 10 ** (3 * (v - 1)));
const rampSet = (mode: 'vinyl' | 'fade') => (v: number) => {
  tape.rampAt = mode === 'fade' ? v : (v - RAMP_LOW) / (1 - RAMP_LOW);
  if (mode === 'fade') {
    rampGain = heard(v);
    applyVolume();
  } else if (audio) {
    audio.preservesPitch = false;
    audio.playbackRate = Math.max(RAMP_LOW, v);
  }
};
/** The ramp's value for a share of the way up (a vinyl's speed, a fade's level). */
const levelAt = (mode: 'vinyl' | 'fade', share: number) => (mode === 'fade' ? share : RAMP_LOW + (1 - RAMP_LOW) * share);
/** Winds down what plays (nothing to do when it is at once, or not playing), from `share` of the way up (a wind-up
 *  turned round part way: only that much of the ramp is left). */
async function rampOut(share = 1): Promise<void> {
  const { mode } = ss();
  if (mode === 'off' || !audio || audio.paused) return;
  // A vinyl's wind-down is the deck's one brake (the owner, 2026-10-27): Tape winding's Brake, its length and its curve
  // (a raised cosine on the speed, as a wind brakes); a fade's is the ramp's.
  // Start and stop's own stop (a pause, a skip's wind-down); a turn of the tape's (a hold's stop, the knob) the tape's
  // Brake (turnRamp's).
  const full = ss().stopMs;
  const ms = full * share;
  rampBegins('out', ms, full, !tape.quiet);
  const at = ramping.at;
  try {
    await ramp(levelAt(mode, share), mode === 'fade' ? 0 : RAMP_LOW, ms, rampSet(mode), DOWN[mode]);
  } finally {
    // Not where another ramp took over (a press turning it round): that one's.
    if (ramping.at === at) rampEnds();
  }
}
/** Plays, winding up from still (or silence); not while a ramp is under way. `early`: a stream begun that much (`lead`
 *  s) before the point it winds up to, over `ms` (playInto). */
function rampIn(early?: { ms: number; lead: number }) {
  if (!audio) return;
  if (tape.committed === true) return turnUp();
  if (tape.committed !== null) return;
  const { mode } = ss();
  let { ms } = ss();
  // From a track's start it plays at once, always (the owner, 2026-10-26): there is nothing to wind up from.
  if (mode === 'off' || (!early && (where()?.offset ?? 0) < FROM_START)) {
    unramp();
    void audio.play().catch(refused);
    return;
  }
  tape.committed = false;
  local.paused = false;
  // The track time the wind-up takes (spanOf, from its own curve): it starts that much early, so it is at full speed at
  // the press, within the track and what is loaded.
  const saved = audio.currentTime;
  const w = where(saved);
  // No longer than there is track before it (the owner, 2026-10-26: a 3 s fade 1 s in is a 1 s fade).
  const full = ms;
  if (early) {
    ms = early.ms;
    tape.heldAt = saved + early.lead;
  } else if (tape.quiet) {
    // The tape's own (after a turn, a wind): it spins up from where it stands, as a tape does, nothing heard twice (the
    // owner, 2026-10-27: a direction change sounded wrong; the effect's way starts early to be at full speed there).
    ms = windUpMs(ms, mode, w ? w.offset : 0);
  } else {
    ms = windUpMs(ms, mode, w ? w.offset : 0);
    const start = Math.max(w ? w.seg.start : 0, saved - spanOf(mode, 'in', ms));
    tape.heldAt = saved;
    if (inBuffer(start)) audio.currentTime = start;
  }
  const set = rampSet(mode);
  set(mode === 'fade' ? 0 : RAMP_LOW);
  rampBegins('in', ms, full, !tape.quiet);
  if (tape.quiet) playback('info', 'tape on', { index: local.index, at: Math.round((where()?.offset ?? 0) * 1000) });
  // A vinyl's start at its slowest, faded in over its first moment (no plop); a fade starts silent anyway.
  if (mode === 'vinyl') {
    rampGain = 0;
    applyVolume();
    void gainTo(1, DECLICK_MS);
  }
  void audio.play().catch(refused);
  const mine = ++run;
  void ramp(mode === 'fade' ? 0 : RAMP_LOW, 1, ms, set, UP[mode]).then(() => {
    if (mine !== run) return;
    rampEnds();
    tape.heldAt = null;
    unramp();
    tape.committed = null;
  });
}
/** A wind-down turned round by play (the owner, 2026-10-27): back up from where it has got to, over what is left of the
 *  ramp, playing on from where it sounds (the counter goes with it from the press). */
function turnUp() {
  if (!audio || tape.committed !== true) return;
  nextKept = pendingNext.pauseDone(nextKept);
  const { mode } = ss();
  if (mode === 'off') return atOnce();
  const share = Math.max(0, Math.min(1, tape.rampAt));
  tape.committed = false;
  local.paused = false;
  tape.heldAt = null;
  const ms = ss().ms * (1 - share);
  rampBegins('in', ms, ss().ms, !tape.quietPause);
  tape.quietPause = false;
  const mine = ++run;
  void ramp(levelAt(mode, share), 1, ms, rampSet(mode), UP[mode]).then(() => {
    if (mine !== run) return;
    rampEnds();
    unramp();
    tape.committed = null;
  });
}
/** Whatever ramp runs ends here, at full speed and volume (a switch between devices has no effects; the owner,
 *  2026-10-27). */
function atOnce() {
  if (tape.committed === null) return;
  ++run;
  tape.heldAt = null;
  unramp();
  rampEnds();
  tape.committed = null;
}
/** Pauses, winding down first; paused at once as far as anyone sees. During a wind-up it turns round from where that
 *  has got to, resting where the wind-up was going (the counter never left it). Play meanwhile turns it up again. */
async function pauseRamped() {
  if (!audio || tape.committed === true) return;
  const turning = tape.committed === false && ramping.dir === 'in';
  if (tape.committed !== null && !turning) return;
  if (ss().mode === 'off' || audio.paused) {
    atOnce();
    return void audio?.pause();
  }
  const share = turning ? Math.max(0, Math.min(1, tape.rampAt)) : 1;
  tape.committed = true;
  tape.quietPause = tape.quiet;
  local.paused = !tape.quiet;
  // The moment it was pressed: the counter stays there while the end sounds, and it rests there after.
  if (!turning || tape.heldAt === null) tape.heldAt = audio.currentTime;
  // The tape's own stop: its counter goes with the tape to where it stands (only an effect's pause holds the press's).
  if (tape.quietPause) tape.heldAt = null;
  const mine = ++run;
  try {
    await rampOut(share);
    if (mine !== run) return;
    // Never stopped at once at the brake's slowest (a plop, the owner's tablet 2026-10-27): its last moment faded.
    await gainTo(0, DECLICK_MS);
    if (mine !== run) return;
    audio?.pause();
    if (tape.quietPause) playback('info', 'tape stood', { index: local.index, at: Math.round((where()?.offset ?? 0) * 1000) });
    // A pause rests where it was pressed (the effect's way); the tape's own stop (a turn, a hold's) where the tape came
    // to stand, so what follows goes on from the very place the sound ended.
    if (audio && !tape.quietPause && tape.heldAt !== null && inBuffer(tape.heldAt)) audio.currentTime = tape.heldAt;
    // Next kept from the brake: from the standstill, the next track starts up.
    if (tape.nextAfterPause && local.index + 1 < local.queue.length) jump(local.index + 1);
  } finally {
    nextKept = pendingNext.pauseDone(nextKept);
    if (mine === run) {
      tape.heldAt = null;
      unramp();
      tape.committed = null;
      tape.quietPause = false;
    }
  }
}
/** For next and previous (where the device asks it): out only, and only then on to `go` (the track, its cover), which
 *  starts as it is, at full speed and volume (the owner, 2026-10-26: no wind-up on a skip). */
async function skipRamped(go: () => void) {
  const s = ss();
  // During a play or pause ramp: on at once (anything interrupts anything, the owner 2026-10-27).
  if (tape.committed !== null && !tape.rewinding) {
    atOnce();
    return go();
  }
  if (tape.committed !== null) return;
  if (s.mode === 'off' || !s.skips || !audio || audio.paused || handed()) return go();
  tape.committed = false;
  // The counter stays where it was pressed until the change (as the app does).
  tape.heldAt = audio.currentTime;
  const mine = ++run;
  try {
    // Not a stop of the music (it plays on, the next): no grey, no drained button (the owner, 2026-10-27).
    await quietly(() => rampOut());
  } finally {
    if (mine === run) {
      tape.heldAt = null;
      tape.committed = null;
    }
  }
  if (mine !== run) return;
  unramp();
  go();
}
/** A seek during a ramp: the ramp stops where it is. Winding up, it jumps at once and comes to full speed and volume
 *  over SETTLE_MS after the jump (no gap before it, no click); winding down, it stops (an 8 ms fade, no click) and rests
 *  at the new place. */
const SETTLE_MS = 30;
async function overrideRamp(secs: number) {
  if (!audio) return;
  const pausing = tape.committed === true;
  run++;
  rampToken++;
  rampEnds();
  tape.heldAt = null;
  tape.committed = null;
  if (pausing) {
    await gainTo(0, 8);
    audio.pause();
    unramp();
    jumpTo(secs);
    return;
  }
  const rate = audio.playbackRate;
  jumpTo(secs);
  const t0 = performance.now();
  const g0 = rampGain;
  await new Promise<void>((done) => {
    const step = () => {
      const t = Math.min(1, (performance.now() - t0) / SETTLE_MS);
      if (audio) {
        audio.playbackRate = rate + (1 - rate) * t;
        rampGain = g0 + (1 - g0) * t;
        applyVolume();
      }
      if (t < 1) setTimeout(step, 5);
      else done();
    };
    step();
  });
  unramp();
}
/** How long a sound's very start and end are faded (ms): never a step, so never a plop. */
const DECLICK_MS = 15;
/** The ramp's gain to `to` over `ms`. */
function gainTo(to: number, ms: number): Promise<void> {
  const g0 = rampGain;
  const t0 = performance.now();
  return new Promise((done) => {
    const step = () => {
      const t = Math.min(1, (performance.now() - t0) / ms);
      rampGain = g0 + (to - g0) * t;
      applyVolume();
      if (t < 1) setTimeout(step, 2);
      else done();
    };
    step();
  });
}
/** How far before a loaded range's start still counts as in it (s): a stream's first audio begins a hair after its
 *  place (an MP3's first frame), and a seek there plays from it (measured 2026-10-28: a wind's landing at 0 read as not
 *  loaded, fetched again, 1.4 s of silence). */
const BUFFER_SLACK_S = 0.1;
/** Whether stream time `t` is loaded. */
function inBuffer(t: number) {
  if (!audio) return false;
  for (let i = 0; i < audio.buffered.length; i++) if (t >= audio.buffered.start(i) - BUFFER_SLACK_S && t <= audio.buffered.end(i)) return true;
  return false;
}
/** Within this much of a track's start (s), play is never ramped. */
const FROM_START = 0.5;
/** How long a wind-up from `offset` s into the track takes: its set `ms`, but no longer than the track before it
 *  allows (at a vinyl's pace, two thirds of full speed on average); none from the very start (the play button shows
 *  this before it starts, lib/NowPlaying). */
export function windUpMs(ms: number, mode: 'off' | 'vinyl' | 'fade', offset: number, low = RAMP_LOW): number {
  if (mode === 'off' || offset < FROM_START) return 0;
  return Math.min(ms, (offset / spanOf(mode, 'in', 1000, low)) * 1000);
}
// --- Winding (decision 0074; the owner, 2026-10-27) ---
// The tape wound back or forward on the server's wind stream (/api/stream/…/wind: the track played that way at the
// speed limit, quieter by the wind volume), on a second player whose own speed is the envelope (lib/windShape): spun up
// over the attack, braked over the release, the pitch following. A tap of Previous winds back to the track's start,
// condensed to its length (cut into heard pieces where the way is long), and so does a seek where seeking winds the
// tape; holding Previous or Next pauses (Start and stop as a pause) while its stream is already fetched, winds for as
// long as it is held, stops at the track's edge or winds on through the queue (At a track's edge), and on release
// brakes and plays on from where it got to, winding up. The counter and the waveform go
// with it: a condensed wind along the straight line from where it began to where it goes, as far as its envelope has
// come; a held one at the speed limit's pace, as far as its stream has played. Not sounding in WIND_WAIT_MS, or failing
// first: a tap or a seek goes there plainly, a hold let go plays on where it was, and a hold still held or the knob's
// backwards asks once more and waits WIND_RETRY_MS, then rests where it stood (lib/player/knob notSounding; the owner,
// 2026-10-30: a hold back whose stream came late played on forwards, the finger still down). Nothing of it counts as
// listening (the wind stream is no play). Any press takes it over where the counter shows it (`windTakeover`); a seek
// ends it and goes there; another queue or track ends it.
const WIND_WAIT_MS = 1500;
/** Asked again, how long it may take: a busy server answered in up to 7 s (the ThinkPad's log, 2026-10-30). */
const WIND_RETRY_MS = 6000;
/** How often its speed and the counter follow it (the tick's four times a second is too coarse to see it go). */
const WIND_STEP_MS = 40;
/** The slowest the browser plays with sound (lib/rates): a spin-up starts there and a brake ends there. */
const RATE_LOW = RATE_FLOOR;
/** A forward hold while playing flies (the owner, 2026-10-27: no stop on the way): its stream from FLY_LEAD s ahead at
 *  FLY_SPEED×, played at 1/FLY_SPEED, so it takes over at exactly the track's own speed and spins up from there (as the
 *  app does). */
const FLY_SPEED = 4;
const FLY_LEAD = 0.5;
/** The fastest a browser plays a track (×): a forward hold let go is handed back to the track's own player at its speed
 *  (never above this: HOLD_TOP), which then brakes to 1× on the tape's Brake; the whole brake is the track, its own
 *  sound (the owner, 2026-10-28: the wind's stream sounded faded as it slowed, its highs lost at speed). */
const TRACK_TOP = RATE_TOP;
/** A hold let go backwards: the backwards player's stream readied this long behind the tape (s at its speed), taken over
 *  as the tape reaches it, the wind sounding on meanwhile. */
const PREP_AHEAD_S = 0.8;
/** A forward hold let go: the track's stream readied where the brake will have got to this far into it (s). */
const HAND_FETCH_S = 0.3;
/** A place in the queue: which track, s into it. */
type Place = { index: number; pos: number };
/** A hold let go: its one plan (the owner, 2026-10-28: "make it pure"). From `start`, at `v0`× braking to `to`× on one
 *  raised cosine over `ms` from `b0`, along the queue places `order` the way `dir` goes, `D` s of track, to `aim`, the
 *  look-ahead. What shows meanwhile is the plan's place; each head that sounds (the wind, the track's player, the
 *  backwards player) plays at the plan's speed, put right by how far it is from the plan's place (planSpeed); every
 *  handover takes place at the plan's place; the brake ends at its aim. A browser's late rate changes (about 0.1 s) and
 *  its clock's jumps under load (the tablet's: 7 s of track at once at 128×) then never move where it lands. */
type LetGo = { v0: number; to: number; b0: number; ms: number; start: Place; order: number[]; dir: 1 | -1; D: number; aim: Place };
/** How fast a head is put back on the plan: what it is off, made up over this long. */
const PLAN_CATCH_S = 0.15;
/** `d` s on from `place` the way `dir` goes along the queue places `order`, past a track's edge into the next. */
function along(order: number[], dir: 1 | -1, place: Place, d: number): Place {
  let k = order.indexOf(place.index);
  let { index, pos } = place;
  for (;;) {
    const len = lengthOf(index) ?? Infinity;
    const room = dir < 0 ? pos : len - pos;
    if (d <= room || k < 0 || k + 1 >= order.length) return { index, pos: Math.max(0, Math.min(len, pos + dir * Math.min(d, room))) };
    d -= room;
    index = order[++k];
    pos = dir < 0 ? (lengthOf(index) ?? 0) : 0;
  }
}
/** How far a plan's way has come from its start to `place` (s of track; null: a place off its way). */
function wayTo(c: LetGo, place: Place): number | null {
  if (place.index === c.start.index) return (place.pos - c.start.pos) * c.dir;
  const a = c.order.indexOf(c.start.index);
  const b = c.order.indexOf(place.index);
  if (a < 0 || b < a) return null;
  const len = (i: number) => lengthOf(c.order[i]) ?? 0;
  let d = c.dir > 0 ? len(a) - c.start.pos : c.start.pos;
  for (let i = a + 1; i < b; i++) d += len(i);
  return d + (c.dir > 0 ? place.pos : len(b) - place.pos);
}
/** The plan's place at `now`. */
const planAt = (c: LetGo, now: number): Place => along(c.order, c.dir, c.start, brakeTravel(c.v0, c.ms, Math.max(0, (now - c.b0) / 1000), c.to));
/** The speed a head at `place` plays at now (×, the track's): the plan's, and what it is off the plan's place made up
 *  over PLAN_CATCH_S; never backwards. */
function planSpeed(c: LetGo, now: number, place: Place): number {
  const v = curveSpeed(c, now, c.ms);
  const got = wayTo(c, place);
  if (got === null) return v;
  const want = brakeTravel(c.v0, c.ms, Math.min(c.ms / 1000, Math.max(0, (now - c.b0) / 1000)), c.to);
  return Math.max(0, v + (want - got) / PLAN_CATCH_S);
}
const curveSpeed = (c: { v0: number; to: number; b0: number }, now: number, ms: number) => brakeSpeed(c.v0, c.to, ms, now - c.b0);
/** When such a brake is down to the fastest a track plays (s into it; 0 where it starts within it). */
function withinReach(v: number, ms: number, to = 1): number {
  if (v <= TRACK_TOP) return 0;
  return (Math.acos((2 * (TRACK_TOP - to)) / (v - to) - 1) / Math.PI) * (ms / 1000);
}
/** How long a flown hold's hand-back waits for the track's stream there before landing the plain way: while the
 *  stream prepared for it is coming, up to LAND_WAIT_MS; otherwise LAND_QUICK_MS. */
const LAND_WAIT_MS = 2000;
const LAND_QUICK_MS = 400;
type Winding = {
  clip: HTMLAudioElement;
  seg: Segment;
  /** Where it began and where it goes (s into the track); `to` null: held, open-ended. */
  from: number;
  to: number | null;
  dir: 1 | -1;
  /** A condensed wind's length (ms). */
  D: number;
  wind: Wind;
  /** When it began to sound (performance.now()), and when its brake began (a held one, let go). */
  at: number | null;
  brake: number | null;
  /** Its speed as the brake began. */
  brakeFrom: number;
  /** Its plain brake's length (ms): a held one's from its speed (startStop's brakeLengthMs), else the Brake as set
   *  (null); a let-go's is its plan's. */
  brakeMs: number | null;
  /** A held one's stream ran out: at the track's edge. */
  edge: boolean;
  /** Its sound's fade (0..1): in as it starts, out as it ends (no plop). */
  fade: number;
  /** A cut one's plan (map=1), where it has come: where in the track each moment of its sound is; and the speed its
   *  stream runs at (a flown one's floor: 1 over it). */
  map: [number, number, number][] | null;
  mapSpeed: number | null;
  /** A hold's getting faster (the tape's Acceleration, its Top speed; the server renders it, the accel_s and max). */
  accelS: number;
  top: number;
  /** Its stream's speed at its start, and whether it gets faster (a hold's); flown: taken over from play
   *  at 1× and handed back moving. */
  v0: number;
  accel: boolean;
  fly: boolean;
  /** A hold let go to play: handed back moving, as soon as the other head is in there (forwards the track's own player;
   *  backwards the backwards player, its stream armed in `handArmed`). */
  handing: boolean;
  handArmed: Armed | null;
  /** Let go: the queue place its brake gets within the track's reach in, where the track is readied (null: not let go). */
  landIndex?: number | null;
  /** A hold let go: its one brake, begun at once: from `v0`× (the flight speed) to `to`× (forwards the track's own 1×,
   *  backwards a stand) on a raised cosine over the tape's Brake from `b0`; whichever head plays it on, the same curve. */
  letGo: LetGo | null;
  /** A hold's brake handed over to the backwards player: the speed it takes over at (× its 1× stream), braking at once. */
  startAt: number | null;
  /** The finger's drag as it is followed (smoothed): the hold's speed held back toward its starting speed by it. */
  drag: number;
  /** Played backwards at 1× by the direction knob (as loud as the music), not a wind; and what follows its brake. */
  reverse: boolean;
  /** A hold flown from playing backwards (the knob at L): let go, it brakes to 1× and plays on backwards. */
  backTo?: boolean;
  after: 'play' | 'pause';
  /** A held one wound on through the queue (At a track's edge): the queue places after this track's edge, in winding
   *  order (the stream joins them seamlessly, at the same speed), and the one it shows now. */
  through: number[];
  shown: number;
  timer: ReturnType<typeof setInterval> | undefined;
};
export { rewindNow };
/** Whether a track can be wound: the server renders the wind stream from its own files, so not a Bandcamp preview nor
 *  the device's own music (the owner, 2026-10-27: there it degrades gracefully, keeping what it can). */
export const windable = (t: { source: string } | null | undefined) => !!t && t.source !== BANDCAMP && t.source !== DEVICE;
/** A hold on a track that cannot be wound: Next speeds the track's own player up (to PLAIN_TOP×, its pitch following),
 *  Previous does nothing while held and is a plain Previous when let go (as the app does). */
const PLAIN_TOP = 4;
/** The wind as loud as the track: the page's volume and the track's level (the wind volume is the stream's own). */
function clipVolume() {
  if (!tape.rewinding || !audio) return;
  // The page's volume and the track's level (not the track's own ramp, which stands still meanwhile), times its own fade.
  const page = (pageVolume.level / 100) * (pageVolume.ducked ? DUCKED : 1);
  tape.rewinding.clip.volume = Math.max(0, Math.min(1, page * (level ? (told?.level ?? 1) : 1) * tape.rewinding.fade));
}
/** A wind's sound faded in from silence over DECLICK_MS (no plop as it starts). */
function fadeIn(w: Winding) {
  const t0 = performance.now();
  const step = () => {
    if (tape.rewinding !== w) return;
    w.fade = Math.min(1, (performance.now() - t0) / DECLICK_MS);
    clipVolume();
    if (w.fade < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
/** A queue place's length, where known (s). */
const lengthOf = (i: number) => local.queue[i]?.duration_secs ?? null;
/** The queue places a held wind may go on through, past the track's edge, in winding order: up to WIND_THROUGH,
 *  stopping at one the server cannot wind (a Bandcamp preview, the device's own music) or whose length is unknown (the
 *  counter goes by lengths: the page cannot read the stream's map). */
const WIND_THROUGH = 64;
function throughOf(index: number, dir: 1 | -1): number[] {
  const out: number[] = [];
  for (let i = index + dir; i >= 0 && i < local.queue.length && out.length < WIND_THROUGH; i += dir) {
    const t = local.queue[i];
    if (t.source === BANDCAMP || t.source === DEVICE || !lengthOf(i)) break;
    out.push(i);
  }
  return out;
}
/** Where a wind has got to: the queue place and the seconds into it, as the counter shows it. A held one has gone the
 *  speed limit's pace times the stream it has played: within its track, then on through the queue (back: each from its
 *  end; forward: each from its start), resting at the last one's edge. */
/** The stream's speed at `s` seconds into it (× the track's). */
function vAt(w: Winding, s: number): number {
  return w.accel ? Math.min(w.top, w.v0 * 2 ** (s / w.accelS)) : w.v0;
}
/** How far through the track(s) the stream has come at `s` seconds into it (s): an accelerating one's integral. */
function distance(w: Winding, s: number): number {
  // A cut one settled (its plan dropped, settle): it stays where it was while it brakes.
  if (w.v0 === 0) return 0;
  if (!w.accel || w.v0 >= w.top) return w.v0 * s;
  const A = w.accelS;
  const reach = A * Math.log2(w.top / w.v0);
  const grown = (u: number) => ((w.v0 * A) / Math.LN2) * (2 ** (u / A) - 1);
  return s <= reach ? grown(s) : grown(reach) + w.top * (s - reach);
}
function windPlace(w: Winding, now: number): { index: number; pos: number } {
  const index = w.seg.index;
  if (w.at === null) return { index, pos: w.from };
  if (w.to !== null) {
    const f = w.fly ? 1 / (w.mapSpeed ?? w.v0) : 0;
    const el = Math.min(w.D, now - w.at);
    const C = f * w.D + (1 - f) * consumed(w.wind, w.D, w.D);
    const share = C > 0 ? Math.min(1, (f * el + (1 - f) * consumed(w.wind, w.D, el)) / C) : 1;
    return { index, pos: w.from + (w.to - w.from) * share };
  }
  return placeAt(w, w.clip.currentTime);
}
/** Where a held wind's stream `s` s in has brought the tape: within its track, then on through the queue. */
function placeAt(w: Winding, s: number): { index: number; pos: number } {
  const index = w.seg.index;
  const end = lengthOf(index) ?? Infinity;
  const d = distance(w, s);
  const room = w.dir < 0 ? w.from : end - w.from;
  if (d <= room || !w.through.length) return { index, pos: Math.min(end, Math.max(0, w.from + w.dir * d)) };
  let rest = d - room;
  for (const i of w.through) {
    const len = lengthOf(i) ?? 0;
    if (rest <= len) return { index: i, pos: w.dir < 0 ? len - rest : rest };
    rest -= len;
  }
  const last = w.through[w.through.length - 1];
  return { index: last, pos: w.dir < 0 ? 0 : (lengthOf(last) ?? 0) };
}
/** The envelope a wind that way goes by: rewind's, or fast-forward's own (the owner, 2026-10-27), seeking's and the
 *  edges' choices the two's alike. */
const windFor = (m: PointMotion): Wind => pointWind(prefs.startStop, m);
/** Whether a motion winds here (winding on, the motion not off: the owner, 2026-10-28). */
const windsOn = (m: Motion) => windsBy(prefs.startStop, m);
/** A hold that way: its own starting and top speed, the tape's spin-up, brake, volume and acceleration. */
const holdFor = (dir: 1 | -1): HoldWind => holdWind(prefs.startStop, dir, Math.min(HOLD_TOP, RATE_TOP));
/** A wind's speed now (× the stream's): its envelope. */
function windRate(w: Winding, now: number): number {
  if (w.at === null) return RATE_LOW;
  const t = now - w.at;
  let r: number;
  // A flown one: from the track's own speed up, and braked down to the slowest the stream may play at now.
  const floor = w.fly ? (w.brake === null ? 1 / w.v0 : Math.max(RATE_LOW, 1 / vAt(w, w.clip.currentTime))) : 0;
  if (w.to !== null) {
    // A flown one never below the track's own speed (its stream's speed floored, fly=1).
    const f = w.fly ? 1 / (w.mapSpeed ?? w.v0) : 0;
    r = f + (1 - f) * rate(w.wind, w.D, t);
  }
  // Let go: the curve's speed, played at its stream's own (the hold's: its flight speed; the backwards player's: 1×).
  // Let go: the plan's speed where it stands (the backwards player's stream is the track at 1×; a hold's, its own speed).
  else if (w.letGo) r = planSpeed(w.letGo, now, windPlace(w, now)) / (w.reverse ? 1 : vAt(w, w.clip.currentTime));
  else if (w.brake !== null) r = floor + ((w.brakeFrom - floor) * (1 + Math.cos(Math.PI * Math.min(1, (now - w.brake) / brakeLength(w))))) / 2;
  else r = t >= w.wind.attackMs ? 1 : floor + ((1 - floor) * (1 - Math.cos((Math.PI * t) / w.wind.attackMs))) / 2;
  // Held back by the finger (a hold, not yet let go): geometrically, at most down to the hold's starting speed (the owner,
  // 2026-10-28: its own 1× within reach would undo the winding).
  if (w.drag > 0 && w.to === null && !w.reverse && w.brake === null) {
    const v = vAt(w, w.clip.currentTime);
    r *= (Math.min(v, holdFor(w.dir).speed) / v) ** w.drag;
  }
  // The backwards player's stream is the track at 1×: it may play faster (a hold's brake handed over).
  return Math.max(RATE_LOW, Math.min(w.reverse ? RATE_TOP : 1, r));
}
// --- The handover between the tape's two heads (the owner, 2026-10-28: one robust way, not one per motion) ---
// The track's own player and the wind stream never sound together: every motion hands the tape over from one to the
// other at a place. Each handover goes the same three steps. Predict: where it will be (a brake's distance, from its
// speed and the Brake setting: standOf for the track, landingOf for a wind). Prepare: the moment that is known, the other
// head readied there (a wind's stream asked from there, armWind; the track sought there, or its stream fetched from just
// before it, prepare). Take over: at the place, the other head plays from what was prepared (whenThere), never fetching
// it twice (streamFrom); an armed stream is taken only where it begins where the tape stands (startWind).
/** How close counts as the place itself (s): no seek for less. */
const SAME_PLACE_S = 0.03;
/** How far from where the tape stands an armed wind's stream may begin and still be taken (s): a skip that small
 *  beats the wait for a new one (135-180 ms of silence, measured 2026-10-28). */
const ARM_SLACK_S = 0.1;
/** A predicted landing's stream fetched from this much before it (s), so one a little short still plays at once. */
const PREP_LEAD_S = 0.5;
/** How far into a stream being fetched a handover waits for it rather than fetching again (s). */
const PREP_REACH_S = 10;
/** How long a handover waits for its stream to come in before fetching it again (ms). */
const PREP_WAIT_MS = 4000;
/** Where the track's own player will stand once the tape's own stop has braked it from full speed, `after` s from now
 *  (a hold's: its touch's HOLD_S; the knob's: now). */
function standOf(after = 0): { index: number; pos: number } | null {
  const here = where();
  if (!here || !audio) return null;
  // The tape's own stop: a vinyl's brake over the tape's Brake (turned).
  const run = audio.paused ? 0 : after + spanOf('vinyl', 'out', prefs.startStop.tape.brakeMs);
  return { index: here.seg.index, pos: here.offset + run };
}
/** Queue track `index`'s place in the stream (the latest placed, where it was placed again). */
const segmentOf = (index: number) => [...segments].reverse().find((s) => s.index === index);
/** Whether the stream being fetched covers `pos` s into queue track `index` (it is coming: wait for it). */
const comingFor = (index: number, pos: number) =>
  !!streamFrom && streamFrom.index === index && pos >= streamFrom.from - BUFFER_SLACK_S && pos < streamFrom.from + PREP_REACH_S;
/** The track's own player readied at `place` for a handover there: sought where it is loaded (and silent), else its
 *  stream fetched from `lead` s before it (once: a stream already coming for it is left to come). A wind sounding
 *  meanwhile keeps its counter. */
function prepare(place: { index: number; pos: number }, lead = PREP_LEAD_S) {
  if (!audio || !audio.paused) return;
  // Already in the stream (the track playing, or one after it already appended): sought there.
  const seg = segmentOf(place.index);
  if (seg && inBuffer(seg.start + place.pos)) {
    audio.currentTime = seg.start + place.pos;
    return;
  }
  if (comingFor(place.index, place.pos)) return;
  const shown = { index: local.index, position: local.position };
  tape.rewindFetch = true;
  void fresh(place.index, Math.max(0, place.pos - lead), false);
  tape.rewindFetch = false;
  if (tape.rewinding) Object.assign(local, shown);
}
/** Runs `go` once the track's own player stands at `secs` into queue track `index` with it loaded: at once where it
 *  is, once the stream coming for it is in (prepared), else fetched now; a turn's ramp and quietness carried over. */
function whenThere(index: number, secs: number, go: () => void) {
  if (!audio) return;
  const ready = () => {
    const seg = segmentOf(index);
    if (!audio || !seg || !inBuffer(seg.start + secs)) return false;
    // Already there within a few ms (prepared): no seek, so it starts at once (each seek waits).
    if (Math.abs(audio.currentTime - (seg.start + secs)) > SAME_PLACE_S) audio.currentTime = seg.start + secs;
    local.position = secs;
    return true;
  };
  if (ready()) return go();
  if (!comingFor(index, secs)) void fresh(index, secs, false);
  let gen = generation;
  const turn = tape.turnRamp;
  const still = tape.quiet;
  const t0 = performance.now();
  let again = false;
  const poll = () => {
    // Something else began meanwhile (a jump, a stop): it is no longer this handover's.
    if (gen !== generation) return;
    if (!ready()) {
      if (!again && performance.now() - t0 > PREP_WAIT_MS) {
        playback('warn', 'handover: its stream never came', { index, at: Math.round(secs * 1000) });
        again = true;
        void fresh(index, secs, false);
        gen = generation;
      }
      return void setTimeout(poll, 25);
    }
    own = ownRamp(still, turn);
    try {
      go();
    } finally {
      own = NO_OWN_RAMP;
    }
  };
  setTimeout(poll, 25);
}
/** Where a wind braking stands once its brake has run (its stream's time stepped on at the brake's rate). */
function landingOf(w: Winding, now = performance.now()): { index: number; pos: number } {
  if (w.at === null || w.edge || w.brake === null) return windNow(w);
  if (w.to !== null) return { index: w.seg.index, pos: w.to };
  const end = w.brake + brakeLength(w);
  let s = w.clip.currentTime;
  for (let t = now; t < end; t += 10) s += (windRate(w, t) * Math.min(10, end - t)) / 1000;
  return placeAt(w, s);
}
/** Brakes a held wind from now (let go, play pressed, settled, the knob back), its landing prepared at once. A hold's
 *  brake as long as its speed asks (constant deceleration, to 1× flown forwards, else to a stand); a tap's, a seek's,
 *  the edge's and the 1× reverse's the Brake as set. */
function brakeWind(w: Winding) {
  if (w.brake !== null || w.at === null) return;
  const now = performance.now();
  w.brakeFrom = w.edge ? RATE_LOW : windRate(w, now);
  w.brakeMs = w.to === null && !w.edge && !w.reverse ? brakeLengthMs(vAt(w, w.clip.currentTime) * w.clip.playbackRate, w.fly ? 1 : 0, w.wind.releaseMs) : null;
  w.brake = now;
  prepare(landingOf(w, now));
}
/** How long a wind's brake runs (ms): a let-go's plan's, else its own (brakeWind's), else the Brake as set. */
const brakeLength = (w: Winding) => w.letGo?.ms ?? w.brakeMs ?? w.wind.releaseMs;
/** Plays on at `secs` into queue track `index`, winding up as play does (at once from a track's start). */
function playOnAt(index: number, secs: number) {
  whenThere(index, secs, () => rampIn());
}
/** A wind's stream address: from `from`, `dir`, condensed to `to` over `D` ms (a tap, a seek) or held (`to` null), on
 *  through the queue places `through`. */
function windUrl(t: LocalTrack, from: number, to: number | null, dir: 1 | -1, D: number, through: number[], wind: Wind, accel: { accelS: number; top: number } | null = null): string {
  const ask = new URLSearchParams({ from_ms: String(Math.floor(from * 1000)), dir: dir < 0 ? 'back' : 'forward', speed: String(wind.speed), gain_db: String(wind.gainDb) });
  // A hold: getting faster to its heard top, and past it by cutting to its flight top (flight_max).
  if (accel) for (const [k, v] of Object.entries({ accel_s: accel.accelS, max: accel.top, flight_max: FLIGHT_TOP, piece_ms: PIECE_MS })) ask.set(k, String(v));
  if (to !== null)
    for (const [k, v] of Object.entries({ to_ms: Math.floor(to * 1000), fit_ms: D, attack_ms: wind.attackMs, release_ms: wind.releaseMs, piece_ms: wind.pieceMs })) ask.set(k, String(v));
  if (through.length) ask.set('through', through.map((i) => `${local.queue[i].source}:${local.queue[i].id}`).join(','));
  return `/api/stream/${encodeURIComponent(t.source)}/${encodeURIComponent(t.id)}/wind?${ask}`;
}
/** A wind's player for `url`, loading at once, at the slowest rate, its pitch following its speed. */
function windClip(url: string): HTMLAudioElement {
  const clip = new Audio();
  clip.preload = 'auto';
  clip.preservesPitch = false;
  clip.playbackRate = RATE_LOW;
  clip.src = url;
  clip.load();
  return clip;
}
/** A hold's wind fetched at its press (the owner, 2026-10-27: no wait after the wind-down): its player, from where, the
 *  queue places it goes on through, and which track. */
type Armed = { clip: HTMLAudioElement; from: number; through: number[]; index: number; v0: number; D?: number; url?: string; floor?: number };
/** `lead`, `speed`: a flown hold's, from that far ahead at that speed; `at`: from there, not where the track stands (a
 *  hold flown from playing backwards). */
function armWind(dir: 1 | -1, lead = 0, speed = holdFor(dir).speed, at: Place | null = null): Armed | null {
  const w = where();
  const here = at ?? (w && { index: w.seg.index, pos: w.offset });
  const t = here && local.queue[here.index];
  if (!here || !t || t.source === BANDCAMP || t.source === DEVICE) return null;
  const through = prefs.startStop.edges === 'queue' ? throughOf(here.index, dir) : [];
  const from = here.pos + lead;
  if (from < 0) return null;
  const hold = holdFor(dir);
  const wind = { ...hold, speed };
  const url = windUrl(t, from, null, dir, 0, through, wind, { accelS: hold.doublingS, top: hold.top });
  return { clip: windClip(url), from, through, index: here.index, v0: speed, url };
}
/** A forward wind to `to` while playing, asked from `lead` s ahead (its stream's speed floored at 1×: fly=1). */
function armPoint(to: number, lead: number, motion: PointMotion): Armed | null {
  const here = where();
  const t = here && local.queue[here.seg.index];
  if (!here || !t || !windable(t)) return null;
  const from = here.offset + lead;
  if (to - from < 2) return null;
  const wind = windFor(motion);
  const D = Math.round(tapMs(wind, to - from));
  const url = `${windUrl(t, from, to, 1, D, [], wind, null)}&fly=1`;
  return { clip: windClip(url), from, through: [], index: here.seg.index, v0: wind.speed, D, url };
}
/** A forward wind to a point while playing (a seek forward, Next's fast-forward): no stop on the way (the owner,
 *  2026-10-27: it went down to 0 and up again, where it can stay at 1×). Its stream asked from a moment ahead, taken over
 *  as the track reaches that moment at its own speed, sped up and slowed back to it, and the track handed back moving at
 *  the place. False where it cannot (the caller winds the stopping way). */
function flyTo(to: number, motion: PointMotion, tries = 0): boolean {
  if (!audio || audio.paused || tape.committed !== null || tape.rewinding) return false;
  pointArms.drop();
  const armed = armPoint(to, FLY_LEAD * (tries + 1), motion);
  if (!armed) return false;
  pointArms.arm(armed);
  windAsked(true);
  const giveUp = () => {
    disarm(armed);
    pointArms.release(armed);
  };
  const watch = () => {
    if (tape.pointArmed !== armed) return;
    const w = where();
    if (!audio || audio.paused || !w || w.seg.index !== armed.index || tape.rewinding || tape.committed !== null) {
      giveUp();
      return windAsked(false);
    }
    if (w.offset >= armed.from - 0.02) {
      if (armed.clip.readyState >= 3) {
        pointArms.take();
        // Its floor from the stream as made (way, the app's too): f = (length − area) / (D − area). A server that
        // made it from still (before fly=1) gives about none: then the stopping way.
        const D = armed.D ?? 0;
        const area = consumed(windFor(motion), D, D);
        const len = armed.clip.duration * 1000;
        const f = Number.isFinite(len) && D > area ? (len - area) / (D - area) : 0;
        if (f < 0.05) {
          disarm(armed);
          windAsked(false);
          if (!startWind(to, 1, null, false, false, motion)) jumpTo(to);
          return;
        }
        armed.floor = Math.min(1, f);
        if (!startWind(to, 1, armed, false, true, motion)) {
          disarm(armed);
          windAsked(false);
          jumpTo(to);
        }
        return;
      }
      if (w.offset > armed.from + 0.25) {
        giveUp();
        if (tries < 2 && flyTo(to, motion, tries + 1)) return;
        windAsked(false);
        if (!startWind(to, 1, null, false, false, motion)) jumpTo(to);
        return;
      }
    }
    requestAnimationFrame(watch);
  };
  requestAnimationFrame(watch);
  return true;
}
/** An armed wind not wound after all (let go during the pause, or the track changed). */
function disarm(a: Armed | null) {
  if (!a) return;
  a.clip.removeAttribute('src');
  a.clip.load();
}

/** Starts a wind of the track playing, from where it is: to `to` condensed (a tap, a seek), or held (`to` null) in
 *  `dir`. False where it cannot (nothing there, a ramp or a wind under way, nowhere to go). */
function startWind(to: number | null, dir: 1 | -1, armed: Armed | null = null, reverse = false, fly = false, motion: PointMotion | null = null, startAt: number | null = null, retried = false): boolean {
  if (!audio || tape.committed !== null || tape.rewinding) return false;
  const here = where();
  const t = here && local.queue[here.seg.index];
  if (!here || !t || !windable(t)) return false;
  const { seg } = here;
  let from = here.offset;
  if (to !== null && Math.abs(to - from) < 0.5) return false;
  if (to !== null) dir = to < from ? -1 : 1;
  // Played backwards (the knob): at 1×, as loud as the music; the wind's spin-up and brake.
  // A hold's own speeds; a wind to a point its own; the knob's backwards at 1×, as loud as the music.
  const hold = holdFor(dir);
  // A wind to a point: its own motion's (a seek's where none is said).
  const wind: Wind = reverse ? { ...holdFor(-1), speed: 1, gainDb: 0 } : to === null ? hold : windFor(motion ?? (dir < 0 ? 'seekBack' : 'seekFwd'));
  const D = to === null ? 0 : (armed?.D ?? Math.round(tapMs(wind, to - from)));
  // A hold's stream fetched at its press (`armed`), from where the pause rests: loaded while it wound down.
  // An armed one only where it begins where the tape stands (flown: met in flight, by its own watch).
  const use = armed && armed.index === seg.index && (fly || startAt !== null || Math.abs(armed.from - from) <= ARM_SLACK_S) ? armed : null;
  if (armed && !use) {
    playback('info', 'wind armed elsewhere: asked again', { armed: Math.round(armed.from * 1000), here: Math.round(from * 1000) });
    disarm(armed);
  }
  const through = use ? use.through : to === null && prefs.startStop.edges === 'queue' ? throughOf(seg.index, dir) : [];
  // A hold winds ever faster; the knob's backwards and a tap or a seek keep their speed.
  const accel = to === null && !reverse;
  const url = use ? null : windUrl(t, from, to, dir, D, through, wind, accel ? { accelS: hold.doublingS, top: hold.top } : null);
  const clip = use?.clip ?? windClip(url!);
  if (use) from = use.from;
  const v0 = use?.v0 ?? wind.speed;
  // Flown: it sounds at the track's own speed from its first moment (a wind to a point: its floor as made).
  if (fly) clip.playbackRate = use?.floor ?? 1 / v0;
  // Handed a hold's brake: at the hold's speed from its first moment.
  if (startAt !== null) clip.playbackRate = Math.min(RATE_TOP, startAt);
  const w: Winding = { clip, seg, from, to, dir, D, wind, fade: 0, map: null, mapSpeed: use?.floor ? 1 / use.floor : null, v0, accel, accelS: hold.doublingS, top: hold.top, fly, handing: false, handArmed: null, letGo: null, startAt, drag: 0, reverse, after: 'play', at: null, brake: null, brakeFrom: 1, brakeMs: null, edge: false, through, shown: seg.index, timer: undefined };
  const mine = () => tape.rewinding === w;
  // Every step timed from the press, in the playback log (the owner, 2026-10-27: find any delay at the deepest level).
  const t0 = performance.now();
  const say = (what: string, fields: Record<string, unknown> = {}, lvl: 'info' | 'warn' = 'info') =>
    playback(lvl, `wind ${what}`, { t: Math.round(performance.now() - t0), clip: Math.round(clip.currentTime * 1000), ...fields });
  tape.rewinding = w;
  tape.quietPause = false;
  windAsked(true);
  // A cut one's plan beside its sound (decision 0075: a press during it lands where the sound is; a server before 0.9.13
  // has none, and its counter's line stands in).
  const mapUrl = url ?? use?.url ?? null;
  // A hold's stream flying past its heard top where the server says so (x-flight-max on its map=1 answer; the stream's
  // own headers are not readable from an audio element): its place then follows the flight's cap. An older server caps
  // it at its heard top, as before.
  if (accel && mapUrl)
    fetch(`${mapUrl}&map=1`).then(
      (r) => {
        const f = Number(r.headers.get('x-flight-max'));
        void r.body?.cancel();
        if (mine() && f > w.top) w.top = f;
      },
      () => {},
    );
  if (to !== null && mapUrl)
    windMap(mapUrl).then(
      (m) => {
        if (!mine()) return;
        if (Array.isArray(m?.map)) w.map = m.map;
        if (typeof m?.speed === 'number' && m.speed > 0) w.mapSpeed = m.speed;
      },
      () => {},
    );
  // The counter where it was pressed from the press on, never where the track waits (the owner, 2026-10-27: the
  // waveform jumped to 0 before winding down).
  tape.rewindPos = from;
  local.position = from;
  say('asked', { track: t.id, from: Math.round(from * 1000), to: to === null ? null : Math.round(to * 1000), D, speed: wind.speed, through: through.length });
  for (const ev of ['loadedmetadata', 'canplay', 'waiting', 'stalled'])
    clip.addEventListener(ev, () => mine() && say(ev, { readyState: clip.readyState }, ev === 'waiting' || ev === 'stalled' ? 'warn' : 'info'));
  clipVolume();
  tape.committed = false;
  local.paused = false;
  // Not sounding: there plainly (a tap, a seek), or on where it was (a hold let go).
  const plain = () => {
    endRewind();
    tape.holding = null;
    if (to !== null) return seek(to);
    // The knob's backwards could not start: forwards again, the knob too.
    if (reverse) Object.assign(tapeDir, SKIPPED);
    playOnAt(seg.index, from);
  };
  // Still not sounding after it was asked again (lib/player/knob notSounding): the tape rests where it stood, the knob
  // at R (nothing plays backwards); a hold still held plays on from there once let go.
  const rest = () => {
    endRewind();
    if (reverse) Object.assign(tapeDir, SKIPPED);
    restAt(seg.index, from);
    const h = tape.holding;
    if (h && !h.released) h.rested = { index: seg.index, pos: from };
    else tape.holding = null;
  };
  const notInTime = (why: string, refused = false) => {
    const held = !!tape.holding && !tape.holding.released;
    const next = notSounding({ point: to !== null, handed: fly || startAt !== null, held, reverse, retried, refused });
    say(`not sounding in time: ${next === 'plain' ? 'plainly' : next === 'retry' ? 'asked again' : 'resting'}`, { why, readyState: clip.readyState }, 'warn');
    if (next === 'plain') return plain();
    if (next === 'retry') {
      endRewind();
      if (startWind(null, dir, null, reverse, false, null, null, true)) return;
    }
    rest();
  };
  // Got there: on from there, winding up; past a track's end, the next. In a track wound into, short of its last
  // moment (a fresh stream from its very end would start it from the top).
  const arrive = ({ index, pos }: { index: number; pos: number }) => {
    say('arrived', { index, at: Math.round(pos * 1000), after: w.after });
    // Stood (a hold's, the backwards player's): the tape is where it aimed.
    if (w.to === null && !w.handing) aim(null);
    endRewind();
    const end = lengthOf(index) ?? Infinity;
    // A hold flown from playing backwards (the knob still at L, lib/player/knob afterLetGo): backwards again from where
    // it got to, at 1× (its backwards player missed); the knob turned to R meanwhile: on forwards, as any hold.
    if (w.backTo) {
      tapeDir.state = 'forward';
      if (afterLetGo(dir, true, tapeDir.want, w.after) === 'backwards') {
        const at = Math.max(0, Math.min(pos, end - 0.5));
        restAt(index, at);
        return void untilThere(index, at).then((ok) => ok && tapeDir.want === 'L' && !tape.rewinding && backwards(armBackwards({ index, pos: at })));
      }
    }
    // Settled for something that comes after it (a hold, the knob: settle): resting there, quietly.
    if (!reverse && w.after === 'pause') return restAt(index, Math.max(0, Math.min(pos, end - 0.5)));
    // Played backwards and stopped (a pause, the track's edge, the knob back to R): the tape rests there, or plays on
    // forwards from there (the knob's turn).
    if (reverse) {
      const at = Math.max(0, Math.min(pos, end - 1.5));
      tapeDir.state = 'forward';
      if (w.after === 'pause') return restAt(index, at);
      return turned(() => playOnAt(index, at));
    }
    // Wound to its end: played to its end, then the next.
    if (dir > 0 && pos >= end - 0.5) {
      local.position = Number.isFinite(end) ? end : pos;
      return jump(index + 1);
    }
    // Flown to a point: the track at its own speed from there at once, moving (no stop, no wind-up).
    if (w.fly && w.to !== null && audio)
      return whenThere(index, pos, () => {
        unramp();
        void audio?.play().catch(refused);
      });
    quietly(() => playOnAt(index, index === seg.index ? pos : Math.max(0, Math.min(pos, end - 1.5))));
  };
  // A backward hold let go: the backwards player (its 1× stream, armed just behind the tape) takes over as the tape reaches
  // where it starts, at the hold's speed, and brakes there; then the track spins up forwards (the owner, 2026-10-28: the
  // mirror of the forward one, its whole brake full sound). Missed (not in, or passed), braked on the wind as before.
  let handWaited = 0;
  const handBack = (place: { index: number; pos: number }) => {
    const a = w.handArmed;
    if (!a) return;
    handWaited += WIND_STEP_MS;
    const now = performance.now();
    const c = w.letGo;
    const ran = !c || now - c.b0 >= c.ms;
    if (place.index !== a.index || place.pos < a.from - 0.25 || handWaited > LAND_WAIT_MS || ran) {
      // Missed (not in, passed, or the curve run out): the wind brakes on to its stand.
      say('hand-back missed', { at: Math.round(place.pos * 1000), armed: Math.round(a.from * 1000), ready: a.clip.readyState });
      disarm(a);
      w.handArmed = null;
      w.handing = false;
      return;
    }
    // The plan at the armed place: the backwards player takes it over there, at the plan's speed.
    const speed = Math.min(TRACK_TOP, curveSpeed(c, now, c.ms));
    if (place.pos > a.from + 0.02 || a.clip.readyState < 3) return;
    say('handed back', { index: a.index, at: Math.round(a.from * 1000), speed: Math.round(speed * 100) / 100 });
    w.handArmed = null;
    endRewind();
    if (!startWind(null, -1, a, true, false, null, speed)) {
      disarm(a);
      quietly(() => playOnAt(a.index, a.from));
    } else if (tape.rewinding) tape.rewinding.letGo = c;
  };
  // A flown one braked: the track's own player takes over where it got to, at the speed it got down to, and comes down
  // to 1× (the owner, 2026-10-27: no stop on the way); its stream there first where it is not loaded yet, the wind
  // sounding on meanwhile.
  let landing = false;
  const land = (place: { index: number; pos: number }) => {
    if (landing || !audio) return;
    landing = true;
    let waited = 0;
    const go = () => {
      if (!mine() || !audio) return;
      const now = performance.now();
      const c = w.letGo;
      // Let go: the track takes it over once the plan is within the track's reach, at the plan's place and speed.
      if (c && curveSpeed(c, now, c.ms) > TRACK_TOP) return void setTimeout(go, 16);
      const at = c ? planAt(c, now) : windPlace(w, now);
      const speed = Math.min(TRACK_TOP, c ? curveSpeed(c, now, c.ms) : vAt(w, clip.currentTime) * clip.playbackRate);
      const tw = where();
      // Only where the track is loaded there is it taken over moving; else (a hold wound on through many tracks, its
      // track not in) it lands there at once, as any wind does, never sounding on while the place moves away (the owner,
      // 2026-10-27: a hold over many tracks stuck speeding).
      if (!(tw && tw.seg.index === at.index && inBuffer(tw.seg.start + at.pos))) {
        waited += 50;
        // Its curve run out with the track not in: it lands there (the track spins up once in), never winding on past.
        if (c && now - c.b0 >= c.ms) return arrive(at);
        // Its stream readied ahead and the tape not there yet, or coming in: worth the wait.
        const coming = comingFor(at.index, at.pos) || (!!streamFrom && streamFrom.index === at.index && at.pos < streamFrom.from);
        // Not yet in the track it lands in (readied there, the curve on its way): the wait goes on.
        const toward = w.landIndex ?? place.index;
        const before = w.dir > 0 && at.index < toward;
        if (before ? waited >= 2 * LAND_WAIT_MS : at.index !== toward || waited >= LAND_WAIT_MS || (waited >= LAND_QUICK_MS && !coming)) return arrive(at);
        return void setTimeout(go, 50);
      }
      say('landed', { index: at.index, at: Math.round(at.pos * 1000), speed: Math.round(speed * 100) / 100, since_let_go: c ? Math.round(now - c.b0) : null });
      audio.currentTime = tw.seg.start + at.pos;
      // How long the track's player takes to move from the place it is handed (the aim's check: the curve runs on meanwhile).
      const given = { at: performance.now(), pos: at.pos };
      audio.addEventListener('playing', () => playback('info', 'landed moving', { after_ms: Math.round(performance.now() - given.at), pos: Math.round((where()?.offset ?? 0) * 1000), handed: Math.round(given.pos * 1000) }), { once: true });
      endRewind();
      local.paused = false;
      audio.preservesPitch = false;
      audio.playbackRate = speed;
      void audio.play().catch(refused);
      const back = () => {
        unramp();
        aim(null);
        playback('info', 'back at its own speed', { index: at.index, at: Math.round((where()?.offset ?? 0) * 1000), since_let_go: c ? Math.round(performance.now() - c.b0) : null });
      };
      // A hold let go: the brake that began on the wind goes on on the track, on its own curve, to its end.
      if (c) {
        const token = ++rampToken;
        const on = () => {
          if (token !== rampToken || !audio) return;
          const t = performance.now();
          // The plan's speed where the track stands, at least its own.
          const here = where();
          const v = here ? planSpeed(c, t, { index: here.seg.index, pos: here.offset }) : curveSpeed(c, t, c.ms);
          audio.playbackRate = Math.min(TRACK_TOP, Math.max(1, v));
          if (t - c.b0 >= c.ms) return back();
          setTimeout(on, 16);
        };
        return on();
      }
      void ramp(speed, 1, prefs.startStop.tape.brakeMs, (v) => audio && (audio.playbackRate = v), brakeCurve).then((done) => done && back());
    };
    if (!w.handing) prepare(place);
    go();
  };
  // The counter (and, wound into another track of the queue, the track shown: never heard, so never counted).
  const show = ({ index, pos }: { index: number; pos: number }) => {
    if (index !== w.shown) {
      say('crossed', { index, pos: Math.round(pos * 1000) });
      w.shown = index;
      local.index = index;
      local.duration = lengthOf(index);
    }
    tape.rewindPos = pos;
    local.position = pos;
  };
  const late = setTimeout(() => mine() && w.at === null && notInTime('late'), retried ? WIND_RETRY_MS : WIND_WAIT_MS);
  clip.addEventListener(
    'playing',
    () => {
      if (!mine() || !audio) return;
      clearTimeout(late);
      w.at = performance.now();
      say('playing');
      // A hold's brake handed over: braking from the hold's speed at once, its landing readied.
      if (w.startAt !== null) {
        w.brakeFrom = w.startAt;
        w.brake = w.letGo?.b0 ?? w.at;
        prepare(landingOf(w, w.at));
      }
      fadeIn(w);
      rewindNow.on = true;
      windAsked(false);
      w.timer = setInterval(() => {
        if (!mine()) return clearInterval(w.timer);
        const now = performance.now();
        // The finger's drag followed smoothly while held.
        if (tape.holding) w.drag += (tape.dragWant - w.drag) * (1 - Math.exp(-WIND_STEP_MS / 120));
        if (!w.edge) clip.playbackRate = windRate(w, now);
        // A let-go brake, step by step (the owner, 2026-10-28: not all holds land where the look-ahead said).
        if (w.letGo && !w.edge && !w.reverse) {
          const c = w.letGo;
          playback('info', 'brake step', {
            ms: Math.round(now - c.b0),
            curve: Math.round(curveSpeed(c, now, c.ms) * 10) / 10,
            rate: Math.round(clip.playbackRate * 1000) / 1000,
            plan: Math.round(brakeTravel(c.v0, c.ms, Math.min(c.ms / 1000, (now - c.b0) / 1000), c.to) * 10) / 10,
            wind: Math.round((wayTo(c, windPlace(w, now)) ?? NaN) * 10) / 10,
            D: Math.round(c.D * 10) / 10,
          });
        }
        // Let go: the plan's place (the heads are put on it; where it hands over and ends).
        const place = w.edge ? { index: w.shown, pos: tape.rewindPos ?? w.from } : w.letGo ? planAt(w.letGo, now) : windPlace(w, now);
        show(place);
        // A hold (not the knob's playing backwards, but a hold's brake handed to the backwards player): its speed.
        if (w.to === null && (!w.reverse || w.startAt !== null)) {
          holdNow.on = true;
          holdNow.speed = w.edge ? 0 : vAt(w, clip.currentTime) * clip.playbackRate;
          if (tape.holding) holdNow.dir = w.dir;
          holdNow.held = !!tape.holding && !tape.holding.released;
          holdNow.top = w.top;
          // Held: where letting go now would land; let go, it stays where it aimed (aim) until the tape is there.
          if (tape.holding && !w.edge) {
            const l = letGoLanding(w, place);
            holdNow.landing = l.pos;
            holdNow.landingIndex = l.index;
          }
        }
        // A forward hold let go: to the track's own player, moving.
        if (w.handing && !w.edge) {
          if (w.dir > 0) land(place);
          else handBack(place);
        }
        // A held one braked to its end: on from where it got to (a flown one moving, where it plays on).
        // Let go, the plan's end is its aim: there it stands (backwards) or plays on.
        else if (w.brake !== null && (w.edge || now - w.brake >= brakeLength(w))) {
          // A hold flown from playing backwards, let go and braked to 1× on the backwards player: it plays on backwards,
          // the knob's own again (the owner, 2026-10-30).
          if (w.reverse && !w.edge && w.letGo?.to === 1 && afterLetGo(-1, true, tapeDir.want, w.after) === 'backwards') {
            say('backwards again', { index: place.index, at: Math.round(place.pos * 1000) });
            Object.assign(w, { letGo: null, startAt: null, brake: null, brakeMs: null, landIndex: null });
            holdNow.on = false;
            holdNow.held = false;
            aim(null);
            tapeDir.state = 'backward';
          } else if (w.fly && w.dir > 0 && !w.edge && w.after === 'play') land(place);
          else arrive(place);
        }
      }, WIND_STEP_MS);
      if (!audio.paused) audio.pause();
      // A condensed one's end waits loaded (the counter stays the wind's meanwhile).
      // Its landing readied: the place, or for a wind on to the track's end the next track's start (it goes on there).
      if (to !== null) {
        const end = lengthOf(seg.index) ?? Infinity;
        if (dir > 0 && to >= end - 0.5) {
          if (seg.index + 1 < local.queue.length) prepare({ index: seg.index + 1, pos: 0 }, 0);
        } else prepare({ index: seg.index, pos: to }, 0);
      }
    },
    { once: true },
  );
  clip.addEventListener(
    'ended',
    () => {
      if (!mine()) return;
      if (w.to !== null) return arrive({ index: seg.index, pos: w.to });
      // Settled (its plan dropped): where its counter stands.
      if (w.v0 === 0) return arrive(windPlace(w, performance.now()));
      // Held to the edge (the track's, or the last one's it wound on through): it rests there until let go.
      const { index } = windPlace(w, performance.now());
      const edge = { index, pos: dir < 0 ? 0 : (lengthOf(index) ?? tape.rewindPos ?? w.from) };
      show(edge);
      w.edge = true;
      say('at the edge', { index });
      // Resting at the edge until let go: the track readied there meanwhile.
      prepare(edge, 0);
      // Played backwards to the start: it stops there, paused (the knob stays where it is).
      if (reverse) w.after = 'pause';
      if (w.brake !== null || !tape.holding) arrive(edge);
    },
    { once: true },
  );
  clip.addEventListener(
    'error',
    () => {
      if (!mine()) return;
      say('error', { code: clip.error?.code ?? null, message: clip.error?.message ?? '' }, 'warn');
      clearTimeout(late);
      if (w.at !== null) arrive(windPlace(w, performance.now()));
      else notInTime('error');
    },
    { once: true },
  );
  clip
    .play()
    .then(() => say('play() accepted'))
    .catch((e) => {
      if (!mine() || w.at !== null) return;
      say('play() refused', { error: e instanceof Error ? `${e.name}: ${e.message}` : String(e) }, 'warn');
      clearTimeout(late);
      notInTime('refused', true);
    });
  return true;
}
/** Ends a wind under way (nothing where none is). */
function endRewind() {
  const w = tape.rewinding;
  if (!w) return;
  if (w.handArmed) disarm(w.handArmed);
  w.handArmed = null;
  holdNow.on = false;
  holdNow.held = false;
  // Where it lands is a place of its own, as a seek's (lib/remote: no hold against the last place shown).
  sought.n++;
  clearInterval(w.timer);
  tape.rewinding = null;
  tape.rewindPos = null;
  rewindNow.on = false;
  windAsked(false);
  tape.committed = null;
  // Its last moment faded out before it stops (no plop), what follows already starting meanwhile.
  const clip = w.clip;
  const from = clip.volume;
  const t0 = performance.now();
  const out = () => {
    const u = Math.min(1, (performance.now() - t0) / DECLICK_MS);
    clip.volume = from * (1 - u);
    if (u < 1) return void requestAnimationFrame(out);
    clip.pause();
    clip.removeAttribute('src');
    clip.load();
  };
  requestAnimationFrame(out);
}
/** Holding Previous (`back`) or Next (`forward`) began: winds that way, after pausing as a pause does, until
 *  `windRelease` (the owner, 2026-10-27). The app's player and one of Cratarr's players do it themselves. */
export async function windHold(dir: 'back' | 'forward') {
  if (handed()) {
    holdThere(true);
    return hand('wind', { dir, phase: 'start' });
  }
  if (!audio || local.index < 0 || tape.holding) return;
  // That hold off (the owner, 2026-10-28): nothing while held, the press stays a tap.
  if (!windsOn(dir === 'back' ? 'holdBack' : 'holdFwd')) return;
  // A track that cannot be wound (Bandcamp's, the device's): what can be kept.
  if (!windable(local.queue[local.index])) return startPlainHold(dir === 'back' ? -1 : 1);
  const h: { dir: 1 | -1; released: boolean } = { dir: dir === 'back' ? -1 : 1, released: false };
  tape.dragWant = 0;
  tape.holding = h;
  // Anything under way gives way to it (decision 0075: anything interrupts anything): a wind (not the knob's backwards)
  // brakes to rest where it is, a pause's brake runs out; then the hold, from there.
  if ((tape.rewinding && !tape.rewinding.reverse) || tape.committed === true) {
    await settle();
    if (tape.holding !== h) return void (tape.quietPause = false);
    if (h.released) {
      tape.quietPause = false;
      local.paused = !!audio?.paused;
      return void (tape.holding = null);
    }
  }
  // Playing backwards, the knob at L (the owner, 2026-10-30: holding back did nothing there): a hold back flies on from
  // it, faster and faster, and let go brakes back to playing backwards; a hold forward brakes it to a stop, the knob to
  // R, and winds forwards from there (lib/player/knob holdInReverse).
  if (tape.rewinding?.reverse && tape.rewinding.startAt === null && tape.rewinding.at !== null) {
    if (holdInReverse(h.dir) === 'fly') return flyBack(h);
    if (!(await stopBackwards())) return void (tape.holding === h && (tape.holding = null));
  }
  if (tape.rewinding) return void (tape.holding = null);
  // Forwards while playing forwards: no stop (the owner, 2026-10-27).
  if (h.dir > 0 && !audio.paused && tape.committed === null && tapeDir.want === 'R') return fly(h, 0, takeArmed(1));
  // Fetched at the touch (pre-armed) or now, loading while the pause winds down (it rests where the hold began).
  const stand = standOf();
  const here = where();
  const armed = takeArmed(h.dir, true) ?? armWind(h.dir, stand && here ? stand.pos - here.offset : 0);
  // The tape's own stop: braked as the tape brakes, whatever Start and stop's effect (no fade, no cut: it is no pause).
  if (!audio.paused) await turned(() => pauseRamped());
  if (tape.holding !== h) return disarm(armed);
  if (h.released) {
    disarm(armed);
    tape.holding = null;
    const w = where();
    return void (w && playOnAt(w.seg.index, w.offset));
  }
  if (!startWind(null, h.dir, armed)) {
    disarm(armed);
    tape.holding = null;
    tape.quietPause = false;
  }
}
/** A forward hold while playing: its stream asked from a moment ahead, taken over as the track reaches that moment
 *  (once it is loaded), at 1×. Let go before: nothing changed. Not loaded in time: asked again further on, twice, then
 *  the stopping way. */
function fly(h: { dir: 1 | -1; released: boolean }, tries = 0, pre: Armed | null = null) {
  const armed = pre ?? armWind(1, FLY_LEAD * (tries + 1), FLY_SPEED);
  if (!armed) return void (tape.holding = null);
  const watch = () => {
    if (tape.holding !== h || h.released) {
      if (tape.holding === h) tape.holding = null;
      return disarm(armed);
    }
    const w = where();
    if (!audio || !w || w.seg.index !== armed.index || audio.paused) {
      tape.holding = null;
      return disarm(armed);
    }
    if (w.offset >= armed.from - 0.02) {
      if (armed.clip.readyState >= 3) {
        if (!startWind(null, 1, armed, false, true)) {
          tape.holding = null;
          disarm(armed);
        }
        return;
      }
      if (w.offset > armed.from + 0.25) {
        disarm(armed);
        if (tries < 2) return fly(h, tries + 1);
        tape.holding = null;
        return void windHold('forward');
      }
    }
    requestAnimationFrame(watch);
  };
  requestAnimationFrame(watch);
}
/** A hold back while playing backwards (the knob at L): its stream asked from FLY_LEAD s further back at FLY_SPEED×,
 *  taken over as the backwards play reaches that moment, at 1×, getting faster from there; let go, it brakes back to 1×
 *  and plays on backwards (lib/player/knob letGoTo). Let go before: nothing changed. Not loaded in time: asked again
 *  further on, twice, then nothing (the backwards play goes on). Wound into another track than the one the track's
 *  player stands in: braked to a stop, then the hold from there. */
function flyBack(h: Hold, tries = 0) {
  const r = tape.rewinding;
  if (!r?.reverse) return void (tape.holding = null);
  const here = where();
  if (!here || windNow(r).index !== here.seg.index) return void stopThenHoldBack(h);
  const armed = armWind(-1, -FLY_LEAD * (tries + 1), FLY_SPEED, windNow(r));
  if (!armed) return void (tape.holding = null);
  const watch = () => {
    if (tape.holding !== h || h.released || tape.rewinding !== r) {
      if (tape.holding === h) tape.holding = null;
      return disarm(armed);
    }
    const at = windNow(r);
    if (at.index !== armed.index) {
      tape.holding = null;
      return disarm(armed);
    }
    if (at.pos <= armed.from + 0.02) {
      if (armed.clip.readyState >= 3) {
        playback('info', 'wind from backwards', { index: at.index, at: Math.round(at.pos * 1000), armed: Math.round(armed.from * 1000) });
        endRewind();
        if (!startWind(null, -1, armed, false, true)) {
          disarm(armed);
          tape.holding = null;
          // Resting where the backwards play had got to, the knob at L (the next play backwards).
          tapeDir.state = 'forward';
          return restAt(at.index, at.pos);
        }
        tape.rewinding!.backTo = true;
        return;
      }
      if (at.pos < armed.from - 0.25) {
        disarm(armed);
        if (tries < 2) return flyBack(h, tries + 1);
        tape.holding = null;
        return;
      }
    }
    requestAnimationFrame(watch);
  };
  requestAnimationFrame(watch);
}
/** A hold back while playing backwards in another track than the track's player stands in: braked to a stop there (the
 *  knob at L), then the hold from the stop, back to playing backwards when let go. */
async function stopThenHoldBack(h: Hold) {
  const w = tape.rewinding;
  if (!w?.reverse || w.at === null) return void (tape.holding = null);
  tapeDir.state = 'returning';
  brakeBackwards('pause');
  for (let i = 0; i < 200 && tape.rewinding; i++) await new Promise((r) => setTimeout(r, 25));
  if (tape.holding !== h || tape.rewinding || !(await untilThere(local.index, local.position))) return void (tape.holding === h && (tape.holding = null));
  if (h.released) {
    tape.holding = null;
    return void backwards();
  }
  const armed = armWind(-1);
  if (!startWind(null, -1, armed)) {
    disarm(armed);
    tape.holding = null;
    return void backwards();
  }
  tape.rewinding!.backTo = true;
  tapeDir.state = 'backward';
}
/** Playing backwards braked to a stop for a hold forward (the knob to R): true once the tape rests there, the track's
 *  own player standing there. */
async function stopBackwards(): Promise<boolean> {
  const w = tape.rewinding;
  if (!w?.reverse) return true;
  Object.assign(tapeDir, { want: 'R', state: 'returning' });
  brakeBackwards('pause');
  for (let i = 0; i < 200 && tape.rewinding; i++) await new Promise((r) => setTimeout(r, 25));
  if (tape.rewinding) return false;
  return untilThere(local.index, local.position);
}
/** Brings what runs to rest where it is, quietly (decision 0075: a new action takes over from there): a wind brakes
 *  (a cut one keeps its counter where it was), a pause's brake runs out. Resolves once nothing runs. */
async function settle() {
  const w = tape.rewinding;
  // The tape's own rest between two actions: shown playing (no grey, no fill) until the next takes over.
  tape.quietPause = true;
  tape.quietUntil = performance.now() + 6000;
  if (w && !w.reverse && w.at !== null) {
    tape.holding = null;
    w.after = 'pause';
    if (w.brake === null) {
      const now = performance.now();
      if (w.to !== null) {
        // A cut one's plan dropped: from here a plain brake, from where its sound is.
        const at = soundPlace(w, now);
        w.to = null;
        w.accel = false;
        w.fly = false;
        w.from = at.pos;
        w.v0 = 0;
      }
      brakeWind(w);
    }
  } else if (w && !w.reverse) {
    // Not sounding yet: dropped, resting where it was.
    endRewind();
    restAt(w.seg.index, w.from);
  }
  for (let i = 0; i < 200 && ((tape.rewinding && !tape.rewinding.reverse) || tape.committed === true); i++) await new Promise((r) => setTimeout(r, 25));
}
/** A hold's stream asked at the touch (the owner, 2026-10-27: no wait once it is a hold), for where the hold will start
 *  from: HOLD_S on, as the track plays meanwhile (a forward one also FLY_LEAD further, to be taken over in flight). */
const HOLD_S = 0.35;
export function windArm(dir: 'back' | 'forward') {
  if (handed() || !audio || local.index < 0 || tape.rewinding || tape.committed !== null) return;
  if (!windsOn(dir === 'back' ? 'holdBack' : 'holdFwd')) return;
  windDisarm();
  const d: 1 | -1 = dir === 'back' ? -1 : 1;
  const playingNow = !audio.paused;
  // Flown (forward, playing): met FLY_LEAD on from the hold; stopping first: where that stop will stand.
  const stand = standOf(HOLD_S);
  const here = where();
  const lead = !playingNow ? 0 : d > 0 ? HOLD_S + FLY_LEAD : stand && here ? stand.pos - here.offset : 0;
  const armed = armWind(d, lead, d > 0 && playingNow ? FLY_SPEED : holdFor(d).speed);
  if (armed) preArms.arm({ dir: d, armed, at: performance.now() });
}
/** The touch was a tap after all: its stream dropped. */
export function windDisarm() {
  preArms.drop();
}
/** The pre-armed stream for a hold in `dir`, where it still fits (the same track, not passed yet). `stopping`: the hold
 *  stops first, resting where it began: the stream must start there. */
function takeArmed(dir: 1 | -1, stopping = false): Armed | null {
  const p = preArms.take();
  if (!p) return null;
  const here = where();
  const stand = stopping ? standOf() : null;
  const fits = p.dir === dir && here && here.seg.index === p.armed.index && performance.now() - p.at < 2000 && (stopping ? !!stand && Math.abs(stand.pos - p.armed.from) < ARM_SLACK_S : p.armed.from > here.offset + 0.05);
  if (fits) return p.armed;
  disarm(p.armed);
  return null;
}
/** Where a wind's sound is now, by its plan where it has one (a cut one: the counter glides over the cuts, the sound
 *  does not), else as its counter shows it. */
function soundPlace(w: Winding, now: number): { index: number; pos: number } {
  const m = w.map;
  if (!m?.length || w.to === null || w.at === null) return windPlace(w, now);
  const t = w.clip.currentTime * 1000;
  let k = 0;
  while (k + 1 < m.length && m[k + 1][0] <= t) k++;
  const [s0, , p0] = m[k];
  const next = m[k + 1];
  // Within a piece: on between its ends; at its end (a cut: the next entry starts the next piece), there.
  const pos = next && next[0] > s0 ? p0 + ((next[2] - p0) * (t - s0)) / (next[0] - s0) : p0;
  return { index: w.seg.index, pos: Math.max(0, pos / 1000) };
}
/** Where a wind under way has got: where its sound is, which is where a press during it lands. */
function windNow(w: Winding): { index: number; pos: number } {
  if (w.at === null) return { index: w.seg.index, pos: w.from };
  if (w.edge) return { index: w.shown, pos: tape.rewindPos ?? w.from };
  return soundPlace(w, performance.now());
}
/** Resolves once the track's own player stands at `pos` s into queue track `index` (restAt's seek, or its fetch from
 *  there): true, or false after `ms`. */
async function untilThere(index: number, pos: number, ms = 3000): Promise<boolean> {
  for (let t = 0; t < ms; t += 25) {
    const at = where();
    if (at && at.seg.index === index && Math.abs(at.offset - pos) < 0.15) return true;
    await new Promise((r) => setTimeout(r, 25));
  }
  return false;
}
/** Rests paused at `secs` into queue track `index`. */
function restAt(index: number, secs: number) {
  if (!audio) return;
  whenThere(index, secs, () => audio && !audio.paused && audio.pause());
  local.index = index;
  local.duration = lengthOf(index) ?? local.duration;
  local.position = secs;
  local.paused = !tape.quietPause;
}
/**
 * Any press during a wind takes it over (the owner, 2026-10-27), where the counter shows it: a pause rests there; a
 * play brakes a held one as letting go does and plays on from where it gets (a condensed one plays on from there at
 * once); next and previous skip from there.
 */
function windTakeover(press: 'pause' | 'play' | 'next' | 'previous') {
  const w = tape.rewinding;
  if (!w) return;
  // Played backwards (the knob): a pause brakes it to rest, a play leaves it playing.
  // A hold's brake on the backwards player: a press sets what follows it.
  if (w.reverse && w.startAt !== null && (press === 'pause' || press === 'play')) return void (w.after = press);
  if (w.reverse && (press === 'pause' || press === 'play')) {
    if (press === 'pause' && tapeDir.state === 'backward') {
      tapeDir.state = 'returning';
      brakeBackwards('pause');
    }
    return;
  }
  if (press === 'play' && w.to === null && w.at !== null && !w.edge) {
    tape.holding = null;
    letGo(w);
    return;
  }
  const { index, pos } = windNow(w);
  endRewind();
  tape.holding = null;
  // A hold flown from playing backwards: the tape stands (the knob stays at L, the next play backwards).
  if (w.backTo) tapeDir.state = 'forward';
  if (press === 'pause') return restAt(index, pos);
  if (press === 'play') return playOnAt(index, pos);
  // A skip plays forwards, the knob at R, its own start again too (the owner, 2026-10-30: Previous while playing
  // backwards played the track from its start, the knob left at L).
  Object.assign(tapeDir, SKIPPED);
  if (press === 'next') return jump(index + 1);
  // Previous from where it landed: its own start again past the restart point, else the track before.
  if (pos > RESTART_AFTER) return playOnAt(index, 0);
  jump(Math.max(0, index - 1));
}

/** Holding, the finger dragged down (0..1): the hold's speed held back by it, to aim where it lands (the owner,
 *  2026-10-28). This page's player only. */
export function windDrag(share: number) {
  tape.dragWant = Math.min(1, Math.max(0, share));
}
/** Holding ended: a held wind brakes and plays on from where it got to. */
export function windRelease(dir: 'back' | 'forward') {
  if (handed()) {
    holdThere(false);
    return hand('wind', { dir, phase: 'stop' });
  }
  if (tape.plainHold) return endPlainHold();
  const h = tape.holding;
  if (!h) return;
  h.released = true;
  const w = tape.rewinding;
  if (!w) {
    // Its wind never sounded and the tape rests where it stood: let go, it plays on from there.
    const r = h.rested;
    if (r) {
      tape.holding = null;
      quietly(() => playOnAt(r.index, r.pos));
    }
    return;
  }
  tape.holding = null;
  // Not sounding yet: its wait's fallback plays on.
  letGo(w);
}
/** Where a hold let go now would land, the way letGo lets it go: the tape on to the stream readied ahead of it, then
 *  the brake on the other head (forwards to the track's own speed, backwards to a stand), s into the track. */
function letGoLanding(w: Winding, place: { index: number; pos: number }): { index: number; pos: number } {
  // Braked at once either way: the plan's way, from the speed it flies at (forwards to 1×, back to a stand), as long as
  // that speed asks (constant deceleration).
  const v = vAt(w, w.clip.currentTime) * w.clip.playbackRate;
  const to = letGoTo(w.dir, !!w.backTo, tapeDir.want);
  const ms = brakeLengthMs(v, to, w.wind.releaseMs);
  return onThrough(w, place, brakeTravel(v, ms, ms / 1000, to));
}
/** `d` s on from `place` the way a held wind goes: past its track's edge on through the queue places it winds through
 *  (the owner, 2026-10-28: a brake past the track's end landed in the next one, the look-ahead stuck at the end). */
const onThrough = (w: Winding, place: Place, d: number): Place => along([w.seg.index, ...w.through], w.dir, place, d);
/** Let go: the look-ahead kept where it aimed until the tape is there (the owner, 2026-10-28), at most AIM_KEEP_MS. */
const AIM_KEEP_MS = 4000;
let aimTimer: ReturnType<typeof setTimeout> | undefined;
function aim(at: { index: number; pos: number } | null) {
  clearTimeout(aimTimer);
  holdNow.landing = at?.pos ?? null;
  holdNow.landingIndex = at?.index ?? null;
  if (at !== null) {
    aimTimer = setTimeout(() => ((holdNow.landing = null), (holdNow.landingIndex = null)), AIM_KEEP_MS);
    aimCheck();
  }
}
/** Let go: the look-ahead's line as last painted against where the tape lands (the owner, 2026-10-28): forwards where
 *  it plays at its own speed again, backwards where it stands (its aim the stand, the owner the same day), logged as
 *  `aim vs resume`; given up after AIM_CHECK_MS, or when another hold begins. */
const AIM_CHECK_MS = 12000;
let aimFrame = 0;
function aimCheck() {
  cancelAnimationFrame(aimFrame);
  const from = performance.now();
  const back = holdNow.dir < 0;
  const step = () => {
    const now = performance.now();
    const t = local.queue[local.index];
    // Another hold: this one's check is over.
    if (tape.holding && !tape.holding.released && now - from > 100) return;
    const atOne = back
      ? handed()
        ? deckSpeed(now) === 0 || deckSpeed(now) === 1
        : !tape.rewinding
      : handed()
        ? deckSpeed(now) === 1 && !rewindNow.on
        : !tape.rewinding && !!audio && !audio.paused && Math.abs(audio.playbackRate - 1) < 0.001 && ramping.dir === null;
    if (atOne) {
      const resumed = positionNow();
      const track = t ? `${t.source}/${t.id}` : null;
      const aimed = aimPainted.pos;
      playback('info', 'aim vs resume', {
        way: back ? 'back: stand' : 'forward: 1x',
        aimed: aimed === null ? null : Math.round(aimed * 1000),
        resumed: Math.round(resumed * 1000),
        off_ms: aimed === null || aimPainted.track !== track ? null : Math.round((resumed - aimed) * 1000),
        same_track: aimPainted.track === track,
        painted_ms_ago: Math.round(now - aimPainted.at),
        after_ms: Math.round(now - from),
      });
      return;
    }
    if (now - from < AIM_CHECK_MS) aimFrame = requestAnimationFrame(step);
    else playback('info', 'aim vs resume: never at its own speed', { after_ms: AIM_CHECK_MS });
  };
  aimFrame = requestAnimationFrame(step);
}
/** A held wind let go to play on: forwards, handed back to the track's own player at its speed (readied a moment ahead,
 *  the tape winding on until it is in); backwards (the track plays no other way), braked on the wind and spun up. */
function letGo(w: Winding) {
  if (w.at === null || w.brake !== null || w.handing) return;
  if (w.to !== null || w.reverse || w.edge) return brakeWind(w);
  // Braked at once, as a tape is (the owner, 2026-10-28: a lead before the brake, a guess at the stream's coming, went up
  // to 13 s past the landing shown): one curve from the speed it flies at, forwards down to the track's own speed,
  // backwards to a stand; the other head readied where the curve will be once it is within its reach (and a moment
  // in), and taking the curve over there.
  const now = performance.now();
  const here = windPlace(w, now);
  const speed = vAt(w, w.clip.currentTime) * w.clip.playbackRate;
  const to = letGoTo(w.dir, !!w.backTo, tapeDir.want);
  // As long as its speed asks: constant deceleration, the Brake as set its shortest (the owner, 2026-10-29).
  const T = brakeLengthMs(speed, to, w.wind.releaseMs);
  const landing = letGoLanding(w, here);
  aim(landing);
  playback('info', 'let go', { index: here.index, at: Math.round(here.pos * 1000), speed: Math.round(speed * 100) / 100, landing_index: landing.index, landing: Math.round(landing.pos * 1000) });
  w.letGo = { v0: speed, to, b0: now, ms: T, start: here, order: [w.seg.index, ...w.through], dir: w.dir, D: brakeTravel(speed, T, T / 1000, to), aim: landing };
  w.brakeFrom = 1;
  w.brake = now;
  const ready = Math.max(HAND_FETCH_S, withinReach(speed, T, to));
  // Where the curve will be once within the track's reach: in the queue place it gets to then (the track there readied).
  const reach = onThrough(w, here, brakeTravel(speed, T, ready, to));
  w.landIndex = reach.index;
  if (w.dir > 0) {
    w.handing = true;
    prepare({ index: reach.index, pos: Math.min((lengthOf(reach.index) ?? Infinity) - 1, reach.pos) }, 0);
    return;
  }
  // Backwards: the backwards player's stream (in the track the track's player stands in); none, the wind brakes to its
  // stand and the track spins up from there, readied.
  const armed = reach.index === here.index && here.index === w.seg.index && reach.pos > 1 ? armBackwards(reach) : null;
  if (armed) {
    w.handing = true;
    w.handArmed = armed;
  } else prepare(landing);
}

/** A hold where nothing can be wound: Next while playing speeds the track up on its own player (Start and stop's vinyl
 *  ramp, the wind's spin-up), back to 1× on the brake when let go; anything else waits for the release. */
function startPlainHold(dir: 1 | -1) {
  if (!audio) return;
  const speeding = dir > 0 && !audio.paused && tape.committed === null;
  tape.plainHold = { dir, speeding };
  if (!speeding) return;
  playback('info', 'wind plain', { track: local.queue[local.index]?.id ?? null, dir });
  audio.preservesPitch = false;
  void ramp(1, PLAIN_TOP, holdFor(1).attackMs, (v) => audio && (audio.playbackRate = v), brakeCurve);
}
function endPlainHold() {
  const p = tape.plainHold;
  tape.plainHold = null;
  if (!p || !audio) return;
  if (!p.speeding) return p.dir < 0 ? previous() : next();
  const from = audio.playbackRate;
  void ramp(from, 1, holdFor(1).releaseMs, (v) => audio && (audio.playbackRate = v), brakeCurve).then((done) => done && unramp());
}

// --- The direction knob (the tape deck's; the owner, 2026-10-27) ---
// R plays forwards on the track's own player, L backwards at 1× on the wind stream; the two never sound together, and
// the tape changes direction only from a stop, as a deck does: turned while playing it slows to a stop (a vinyl's ramp,
// whatever Start and stop says), then goes the other way, spinning up. The knob says what is wanted (tapeDir.want); the
// tape's state (tapeDir.state) moves only along forward → stopping → backward → returning → forward, each step taken
// when the one before has come to rest, and only then reading the knob again, so a knob turned back and forth however
// fast always ends in its last place, never in two at once. Paused, the knob only sets the way the next play goes.
// Another track or queue (a skip, a new queue, a jump) plays forwards: the knob goes back to R.
/** Turns the direction knob (lib/TapeDeck): here, or on the app's player or the one driven, which turns itself (its
 *  report confirms it). */
export function turnTape(want: 'L' | 'R') {
  if (!handed() && want === 'L' && !windable(local.queue[local.index])) return;
  if (handed()) {
    tapeDir.want = want;
    return hand('direction', { dir: want });
  }
  tapeDir.want = want;
  void stepTape();
}
/** Plays as the knob says once still, or turns it round: the state machine's one step. */
async function stepTape() {
  if (!audio || local.index < 0) return;
  const playingNow = !local.paused;
  // A wind under way (not the knob's own) comes to rest first, then the knob's way (anything interrupts anything).
  if (tapeDir.state === 'forward' && tapeDir.want === 'L' && tape.rewinding && !tape.rewinding.reverse) {
    await settle();
    if (tapeDir.want === 'L' && backwards()) return;
    // Turned back meanwhile, or nothing to play backwards: on forwards from the rest, quietly.
    tape.quietPause = false;
    const at = where();
    if (at) quietly(() => playOnAt(at.seg.index, at.offset));
    return;
  }
  if (tapeDir.state === 'forward' && tapeDir.want === 'L' && playingNow && !tape.rewinding) {
    tapeDir.state = 'stopping';
    // Its stream asked now, from where the brake will stand, loaded while it brakes (measured on the owner's tablet,
    // 2026-10-28: asked at the stop, 135–180 ms of silence before backwards).
    const asked = armBackwards();
    if (asked) turnArms.arm(asked);
    else turnArms.drop();
    await turned(() => pauseRamped());
    const armed = turnArms.take();
    if (tapeDir.state !== 'stopping') return void disarm(armed);
    tapeDir.state = 'forward';
    if (tapeDir.want === 'L') return void backwards(armed);
    disarm(armed);
    // Turned back meanwhile: forwards again, spinning up.
    const w = where();
    if (w) turned(() => playOnAt(w.seg.index, w.offset));
    return;
  }
  if (tapeDir.state === 'backward' && tapeDir.want === 'R') {
    // A hold flown from playing backwards: its let-go reads the knob (on forwards, lib/player/knob letGoTo).
    if (tape.rewinding?.backTo) return;
    tapeDir.state = 'returning';
    brakeBackwards('play');
  }
}
/** The knob's backwards stream (at 1×, as loud as the music), from where the tape's stop will stand. */
function armBackwards(at: { index: number; pos: number } | null = null): Armed | null {
  const here = at ?? standOf();
  const t = here && local.queue[here.index];
  if (!here || !t || !windable(t)) return null;
  const through = prefs.startStop.edges === 'queue' ? throughOf(here.index, -1) : [];
  const from = here.pos;
  const wind = { ...holdFor(-1), speed: 1, gainDb: 0 };
  return { clip: windClip(windUrl(t, from, null, -1, 0, through, wind, null)), from, through, index: here.index, v0: 1 };
}
/** Plays backwards from where it rests (the knob at L, play pressed or the stop reached); `armed`: its stream asked at
 *  the turn. */
function backwards(armed: Armed | null = null): boolean {
  if (!startWind(null, -1, armed, true)) return false;
  tapeDir.state = 'backward';
  return true;
}
/** Brakes playing backwards to a stop, then `after`: on forwards, or resting there. */
function brakeBackwards(after: 'play' | 'pause') {
  const w = tape.rewinding;
  if (!w?.reverse) return;
  w.after = after;
  if (w.at === null) return void (tapeDir.state = 'forward');
  brakeWind(w);
}
/** Runs `go` with the turn's own ramp (a vinyl's, Start and stop's length where it is one, else the wind's). */
function turned<T>(go: () => T): T {
  const s = prefs.startStop;
  // The tape's own: its spin-up and its Brake (not Start and stop's, which are the effect's).
  own = ownRamp<TurnRamp>(true, { mode: 'vinyl', ms: s.tape.attackMs, stopMs: s.tape.brakeMs });
  try {
    return go();
  } finally {
    queueMicrotask(() => (own = ownRamp(false, own.turn)));
    // A ramp reads it as it begins; one that runs on keeps what it began with.
    queueMicrotask(() => (own = ownRamp<TurnRamp>(own.quiet, null)));
  }
}
/** Play and pause with the knob at L: backwards from rest, or braking to rest. True where it took the press. */
function tapePress(): boolean {
  // Mid-turn (slowing to its stop, or back from playing backwards): the turn finishes first.
  if (tapeDir.state === 'stopping' || tapeDir.state === 'returning') return true;
  if (tape.rewinding?.reverse) {
    // A hold's brake on the backwards player: a press turns what follows it (paused, or on).
    if (tape.rewinding.startAt !== null) {
      tape.rewinding.after = tape.rewinding.after === 'play' ? 'pause' : 'play';
      return true;
    }
    if (tapeDir.state === 'backward') {
      tapeDir.state = 'returning';
      brakeBackwards('pause');
    }
    return true;
  }
  if (tapeDir.want === 'L' && tapeDir.state === 'forward' && audio?.paused && tape.committed === null) {
    backwards();
    return true;
  }
  return false;
}
/** Another track or queue plays forwards: the knob back to R, playing backwards ended. */
function tapeForwards() {
  if (tape.rewinding?.reverse) endRewind();
  Object.assign(tapeDir, { want: 'R', state: 'forward' });
}

/** Cratarr's track as this player keeps it. */
function trackOf(t: Record<string, unknown>): LocalTrack | null {
  if (typeof t.source !== 'string' || typeof t.id !== 'string') return null;
  const lane = t.lane === 'next' || t.lane === 'radio' || t.lane === 'autoplay' ? t.lane : undefined;
  return {
    ...(typeof t.entry === 'string' ? { entry: t.entry } : {}),
    ...(lane ? { lane } : {}),
    source: t.source,
    id: t.id,
    name: typeof t.name === 'string' ? t.name : '',
    artist: typeof t.artist === 'string' ? t.artist : null,
    album: typeof t.album === 'string' ? t.album : null,
    album_id: typeof t.album_id === 'string' ? t.album_id : null,
    ...(typeof t.asked_by === 'string' ? { asked_by: t.asked_by } : {}),
  };
}
const tracksOf = (v: unknown) => (Array.isArray(v) ? v.map((t) => trackOf(t as Record<string, unknown>)).filter((t): t is LocalTrack => !!t) : []);

/** A queue place by entry (decision 0033), else by number; -1 when neither is there. */
function placeOf(m: Record<string, unknown>, entryKey: string, indexKey: string): number {
  const e = m[entryKey];
  if (typeof e === 'string') {
    const at = local.queue.findIndex((t) => t.entry === e);
    if (at >= 0) return at;
  }
  const i = m[indexKey];
  return typeof i === 'number' && i >= 0 && i < local.queue.length ? i : -1;
}

/** One of Cratarr's commands, on this page's own player, as the app's player takes them (PlayOnLink.kt). What a page
 *  cannot do is left: sync play (it joins no group), the measurements. Whether it wants sound from it (a play that a
 *  browser may hold back until the page has been touched once). */
export function playOnCommand(m: Record<string, unknown>): boolean {
  if (!audio) return false;
  switch (m.command) {
    case 'play': {
      const tracks = numbered(tracksOf(m.tracks));
      if (!tracks.length) return false;
      const index = Math.min(Math.max(0, typeof m.index === 'number' ? m.index : 0), tracks.length - 1);
      // Cratarr's queue as it is (shuffled already, its hand-added ones in it): this page keeps none of its own. The one
      // it leaves goes into this browser's list, and the new one is known by Cratarr's id and name.
      const left = standing();
      local.queue = tracks;
      local.shuffled = m.shuffle === true;
      if (local.repeat === 'one') local.repeat = 'off';
      local.from = typeof m.queue_name === 'string' ? m.queue_name : null;
      keepLocal(left, typeof m.queue_id === 'string' ? m.queue_id : newQueueId());
      const from = (typeof m.position_ms === 'number' ? m.position_ms : 0) / 1000;
      // A move between players (or a start here from elsewhere): at once, no Start and stop (the owner, 2026-10-27:
      // switching devices bypasses every effect).
      void fresh(index, from, m.paused !== true);
      fill();
      return m.paused !== true;
    }
    case 'add': {
      const tracks = tracksOf(m.tracks);
      if (!tracks.length) return false;
      const after = typeof m.after === 'string' ? (m.after === '' ? 0 : local.queue.findIndex((t) => t.entry === m.after) + 1 || -1) : -1;
      const at = after >= 0 ? after : typeof m.at === 'number' ? Math.min(Math.max(0, m.at), local.queue.length) : local.queue.length;
      local.queue = [...local.queue.slice(0, at), ...tracks, ...local.queue.slice(at)];
      if (local.index < 0) fresh(0, 0, false);
      else cutFrom(at);
      return false;
    }
    case 'move': {
      const from = placeOf(m, 'entry', 'from');
      const afterAt = typeof m.after === 'string' ? (m.after === '' ? 0 : local.queue.findIndex((t) => t.entry === m.after)) : -1;
      const to = afterAt < 0 ? (typeof m.to === 'number' ? m.to : -1) : m.after === '' ? 0 : afterAt > from ? afterAt : afterAt + 1;
      move(from, to);
      return false;
    }
    case 'remove':
      remove(placeOf(m, 'entry', 'index'));
      return false;
    case 'upcoming': {
      if (typeof m.shuffled === 'boolean') local.shuffled = m.shuffled;
      const keep = local.index + 1;
      local.queue = [...local.queue.slice(0, keep), ...tracksOf(m.tracks)];
      cutFrom(keep);
      return false;
    }
    case 'jump': {
      const at = placeOf(m, 'entry', 'index');
      if (at >= 0) jump(at);
      return at >= 0;
    }
    case 'pause':
      if (tape.rewinding) windTakeover('pause');
      else void pauseRamped();
      return false;
    case 'resume':
      if (tape.rewinding) windTakeover('play');
      else rampIn();
      return true;
    case 'toggle':
      togglePlay();
      return !audio.paused;
    case 'stop':
      atOnce();
      stop();
      return false;
    case 'next':
      next();
      return false;
    case 'previous':
      previous();
      return false;
    case 'seek':
      if (typeof m.position_ms === 'number') seek(m.position_ms / 1000, m.wind === true ? true : undefined);
      return false;
    case 'repeat':
      if (m.mode === 'off' || m.mode === 'one' || m.mode === 'all') setRepeat(m.mode);
      return false;
    case 'radio':
      setRadio(m.on === true, {
        ...(m.by === 'track' || m.by === 'artist' || m.by === 'era' ? { by: m.by as RadioBy } : {}),
        ...(typeof m.count === 'number' ? { count: m.count } : {}),
      });
      return false;
    case 'autoplay':
      if (typeof m.mode === 'string')
        setAutoplay(m.mode as AutoplayMode | 'stop', {
          ...(typeof m.albums === 'boolean' ? { albums: m.albums } : {}),
          ...(typeof m.restart === 'boolean' ? { restart: m.restart } : {}),
          ...(Array.isArray(m.seed) ? { seed: m.seed as string[] } : {}),
          ...(typeof m.within === 'string' || m.within === null ? { within: m.within as string | null } : {}),
        });
      return false;
    case 'skip_favourites':
      setSkipFavourites(m.on === true);
      return false;
    // The tape deck's direction knob turned from another device (the owner, 2026-10-27).
    case 'direction':
      if (m.dir === 'L' || m.dir === 'R') turnTape(m.dir);
      return false;
    // Previous or Next held on another device (decision 0074): wound here as a hold here is. Where this player's hold is
    // off, the press is the plain skip it would be, as the app does (the architecture audit, 2026-10-29: this case was
    // missing, so a hold sent to a page's player was dropped).
    case 'wind': {
      const dir = m.dir === 'forward' ? 'forward' : 'back';
      if (m.phase === 'stop') windRelease(dir);
      else if (windsOn(dir === 'back' ? 'holdBack' : 'holdFwd')) void windHold(dir);
      else if (dir === 'back') previous();
      else next();
      return false;
    }
    // Level the volume set from another device (the owner, 2026-10-27): on or off, and what an unmeasured track gets.
    case 'level':
      if (typeof m.on === 'boolean') setLevelling(m.on);
      if (m.unknown === 'typical' || m.unknown === 'untouched') prefs.levelUnknown = m.unknown;
      return false;
    case 'volume':
      pageVolume.level = Math.min(100, Math.max(0, typeof m.level === 'number' ? m.level : 100));
      applyVolume();
      return false;
    case 'duck':
    case 'unduck':
      pageVolume.ducked = m.command === 'duck';
      applyVolume();
      return false;
    case 'notice':
      // A word for whoever plays here (0.10: a track asked for, accepted or declined; one asked of this player).
      if (typeof m.text === 'string' && m.text) toast.show({ text: m.text });
      return false;
    default:
      // The sleep timer is this page's own (lib/sleep); sync play and its measurements are not a page's.
      return false;
  }
}

if (typeof navigator !== 'undefined' && 'mediaSession' in navigator) lockScreenActions();
