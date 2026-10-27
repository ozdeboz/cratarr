<script lang="ts">
  // Start and stop (the owner, 2026-10-25): play and pause at once, as a turntable or fading, over a ramp; next and
  // previous too where asked; and a tape's rewind for Previous back to a track's start, the track played backwards (the
  // owner, 2026-10-26; back in 0.9.9 as the server's clip). Its rows, the same in This device's Playing
  // (lib/settings/DevicePart) and in the sheet a long press on the play button opens (lib/StartStopSheet, the owner
  // 2026-10-26). The browser's own (prefs), or the app's player's (told to the app, as Level the volume).
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import { prefs, RAMP_MS, type StartStop } from '../prefs.svelte';
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import Item from './Item.svelte';

  const MODES: { value: StartStop['mode']; label: string }[] = [
    { value: 'off', label: 'Off' },
    { value: 'vinyl', label: 'Vinyl' },
    { value: 'fade', label: 'Fade' },
  ];
  const STARTSTOP_MORE =
    "Vinyl: play and pause speed up from still and slow down to still, the pitch following, as a turntable. Fade: they fade in and out. Over the ramp's time. A track running into the next stays seamless.";
  const REWIND_MORE = "Previous, back to the start of the track playing, plays it backwards to there, as a tape: four times the ramp's time, at most 12 s.";
  const ss = $derived(inApp ? appInfo.startStop : prefs.startStop);
  function setSS(change: Partial<StartStop>) {
    const next = { ...ss, ...change };
    if (inApp) {
      appInfo.startStop = next;
      setDevice({ startStop: next });
    } else prefs.startStop = next;
  }
  let rampOpen = $state(false);
  let rampDrag = $state<number | null>(null);
</script>

<Item name="Start and stop" more={STARTSTOP_MORE} experimental><Seg label="Start and stop" options={MODES} bind:value={() => ss.mode, (v) => setSS({ mode: v })} /></Item>
{#if ss.mode !== 'off' || ss.rewind}
  <Item name="Ramp" value={`${(ss.ms / 1000).toFixed(1)} s`} onclick={() => (rampOpen = true)} />
{/if}
{#if ss.mode !== 'off'}
  <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
{/if}
<Item name="Tape rewind" more={REWIND_MORE} experimental toggle={{ checked: ss.rewind, onchange: (on) => setSS({ rewind: on }) }} />
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

  .ramp output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
