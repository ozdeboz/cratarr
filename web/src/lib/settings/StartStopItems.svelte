<script lang="ts">
  // Start and stop (the owner, 2026-10-25): play and pause at once, as a turntable or fading, over a ramp; next and
  // previous too where asked; and a tape's rewind for Previous back to a track's start, the track played backwards (the
  // owner, 2026-10-26; back in 0.9.9 as the server's clip). Its rows, the same in This device's Playing
  // (lib/settings/DevicePart) and in the sheet a long press on the play button opens (lib/StartStopSheet, the owner
  // 2026-10-26). The browser's own (prefs), or the app's player's (told to the app, as Level the volume).
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import { CURVE_START, prefs, RAMP_MS, WIND, type StartStop, type Wind, type WindCurve } from '../prefs.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import Item from './Item.svelte';
  import WindGraph from './WindGraph.svelte';

  const MODES: { value: StartStop['mode']; label: string }[] = [
    { value: 'off', label: 'Off' },
    { value: 'vinyl', label: 'Vinyl' },
    { value: 'fade', label: 'Fade' },
  ];
  const DECK: { value: 'off' | 'effects' | 'always'; label: string }[] = [
    { value: 'off', label: 'Off' },
    { value: 'effects', label: 'Effects' },
    { value: 'always', label: 'Always' },
  ];
  const STARTSTOP_MORE =
    "Vinyl: play speeds up from still over the ramp's time, and pause slows down to still with Tape winding's Brake, the pitch following, as a turntable. Fade: they fade in and out over the ramp's time. A track running into the next stays seamless.";
  const WIND_MORE =
    "Previous and Next wind the tape. A tap of Previous winds back to the track's start, a tap of Next on to its end and then the next track, as a tape spinning up, running and braking; where the way is long, it is cut into short pieces. Holding Previous or Next winds back or forward for as long as you hold, ever faster, and plays on from where you let go. Rewind and fast-forward each have their own curve.";
  const SEC = (v: number) => `${(v / 1000).toFixed(2).replace(/\.?0+$/, '')} s`;
  /** Winding's tunings (the owner, 2026-10-27): an envelope on the tape's speed, each a slider under its graph. */
  const TUNINGS: { key: keyof typeof WIND; label: string; text: (v: number) => string }[] = [
    { key: 'ratio', label: 'Duration', text: (v) => `${v.toFixed(1)} s a minute` },
    { key: 'speed', label: 'Speed limit', text: (v) => `${v}×` },
    { key: 'attackMs', label: 'Spin-up', text: SEC },
    { key: 'releaseMs', label: 'Brake', text: SEC },
    { key: 'pieceMs', label: 'Piece length', text: SEC },
    { key: 'gainDb', label: 'Wind volume', text: (v) => (v === 0 ? 'as played' : `${v} dB`) },
  ];
  const EDGES: { value: Wind['edges']; label: string }[] = [
    { value: 'stop', label: 'Stop' },
    { value: 'queue', label: 'Through the queue' },
  ];
  let rewindOpen = $state(false);
  /** Which direction's curve the sheet shows (the owner, 2026-10-27: each to one's taste). */
  let side = $state<'back' | 'fwd'>('back');
  const SIDES: { value: 'back' | 'fwd'; label: string }[] = [
    { value: 'back', label: 'Rewind' },
    { value: 'fwd', label: 'Fast-forward' },
  ];
  let tuning = $state<{ key: keyof typeof WIND; value: number } | null>(null);
  const ss = $derived(inApp ? appInfo.startStop : prefs.startStop);
  function setSS(change: Partial<StartStop>) {
    const next = { ...ss, ...change };
    if (inApp) {
      appInfo.startStop = next;
      setDevice({ startStop: next });
    } else prefs.startStop = next;
  }
  const setWind = (change: Partial<Wind>) => setSS({ wind: { ...ss.wind, ...change } });
  /** The direction shown's curve set (rewind's in `wind`, fast-forward's in `windFwd`). */
  const setCurve = (change: Partial<WindCurve>) => (side === 'back' ? setWind(change) : setSS({ windFwd: { ...ss.windFwd, ...change } }));
  /** The wind as the sheet shows it: the direction's, a slider being dragged already in its graph. */
  const curve = $derived(side === 'back' ? ss.wind : { ...ss.wind, ...ss.windFwd });
  const shown = $derived(tuning ? { ...curve, [tuning.key]: tuning.value } : curve);
  let rampOpen = $state(false);
  let rampDrag = $state<number | null>(null);
</script>

<Item name="Start and stop" more={STARTSTOP_MORE} experimental><Seg label="Start and stop" options={MODES} bind:value={() => ss.mode, (v) => setSS({ mode: v })} /></Item>
{#if ss.mode !== 'off'}
  <Item name="Ramp" value={`${(ss.ms / 1000).toFixed(1)} s`} onclick={() => (rampOpen = true)} />
  <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
{/if}
<Item name="Tape winding" more={WIND_MORE} experimental toggle={{ checked: ss.rewind, onchange: (on) => setSS({ rewind: on }) }} />
{#if ss.rewind}
  <Item name="Winding" value={`${TUNINGS[0].text(ss.wind.ratio)} · ${TUNINGS[1].text(ss.wind.speed)}`} onclick={() => (rewindOpen = true)} />
  <Item
    name="Seeking winds the tape"
    more="A seek, on the waveform or the clock, winds there instead of jumping, taking as long as a tap would for that distance."
    toggle={{ checked: ss.wind.seekWinds, onchange: (on) => setWind({ seekWinds: on }) }}
  />
  <Item name="At a track's edge" more="Holding past the start or end of a track: stop there, or wind on into the track before or after it in the queue."
    ><Seg label="At a track's edge" options={EDGES} bind:value={() => ss.wind.edges, (v) => setWind({ edges: v })} /></Item
  >
{/if}
<!-- This page's showing (prefs, never the app's): the player's row under the title a tape deck, while an effect runs or
     always (the owner, 2026-10-27). -->
<Item
  name="Tape deck"
  more="The player shows a tape deck where its info row is: its reels turn at the playing speed, slowing for a pause and racing back for a rewind; its head lights on each bar's first beat. During effects: while play or pause ramps or a rewind runs, then the row again."
  ><Seg label="Tape deck" options={DECK} bind:value={() => prefs.tapeDeck, (v) => (prefs.tapeDeck = v)} /></Item
>
{#if rewindOpen}
  <MenuSheet label="Tape winding" title="Tape winding" onclose={() => (rewindOpen = false)}>
    <div class="ramp">
      <div class="side"><Seg label="Direction" options={SIDES} bind:value={side} /></div>
      <WindGraph wind={shown} dir={side} />
      {#each TUNINGS as t (t.key)}
        {@const r = WIND[t.key]}
        {@const v = shown[t.key]}
        <SettingRow label={t.label}>
          <input
            type="range"
            use:panSafe
            min={r.min}
            max={r.max}
            step={r.step}
            value={v}
            aria-label={t.label}
            oninput={(e) => (tuning = { key: t.key, value: Number(e.currentTarget.value) })}
            onchange={(e) => {
              setCurve({ [t.key]: Number(e.currentTarget.value) });
              tuning = null;
            }}
          />
          <output>{t.text(v)}</output>
        </SettingRow>
      {/each}
    </div>
    <MenuEntry icon="undo" label="Reset to defaults" onclick={() => setCurve(CURVE_START)} />
  </MenuSheet>
{/if}
{#if rampOpen}
  <MenuSheet label="Ramp" title="Ramp" chips={[MODES.find((m) => m.value === ss.mode)?.label ?? '']} onclose={() => (rampOpen = false)}>
    <div class="ramp">
      <SettingRow label="Ramp">
        <input
          type="range"
          use:panSafe
          min={RAMP_MS.min}
          max={RAMP_MS.max}
          step="100"
          value={rampDrag ?? ss.ms}
          aria-label="Ramp"
          oninput={(e) => (rampDrag = Number(e.currentTarget.value))}
          onchange={(e) => {
            setSS({ ms: Number(e.currentTarget.value) });
            rampDrag = null;
          }}
        />
        <output>{((rampDrag ?? ss.ms) / 1000).toFixed(1)} s</output>
      </SettingRow>
    </div>
  </MenuSheet>
{/if}

<style>
  /* The ramp's sheet, as Radio's After each track. */
  .ramp {
    padding: 0 16px 16px;
  }

  /* The sheet's one slider, as wide as the sheet (the owner's iPhone, 2026-10-27: a short one under its name). */
  .ramp :global(.control) {
    flex: 1 1 14em;
  }

  .ramp input {
    flex: 1;
    width: auto;
    min-width: 0;
    accent-color: var(--accent);
  }

  .side {
    margin: 4px 0 8px;
  }

  .ramp output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
