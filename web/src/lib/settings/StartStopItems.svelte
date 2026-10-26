<script lang="ts">
  // Start and stop (the owner, 2026-10-25): play and pause at once, as a turntable or fading, over a ramp; next and
  // previous too where asked. Its rows, the same in This device's Playing (lib/settings/DevicePart) and in the sheet a
  // long press on the play button opens (lib/StartStopSheet, the owner 2026-10-26). The browser's own (prefs), or the
  // app's player's (told to the app, as Level the volume). Tape rewind waits for the real thing, the track played
  // backwards (the owner, 2026-10-26: hidden until then).
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
{#if ss.mode !== 'off'}
  <Item name="Ramp" value={`${(ss.ms / 1000).toFixed(1)} s`} onclick={() => (rampOpen = true)} />
  <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
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

  .ramp input {
    width: 11em;
    max-width: 100%;
    accent-color: var(--accent);
  }

  .ramp output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
