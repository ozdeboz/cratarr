<script lang="ts">
  // The tape (the owner, 2026-10-27: one page, in order, not "Start and stop" only): Start and stop's effect; the tape's
  // own mechanics, every motion's alike; its four motions, two settings each (back and forward to a point, holding back
  // and forward); the tape deck. The same groups in This device (lib/settings/DevicePart, its Tape row) and in the sheet
  // a long press on the play button opens (lib/StartStopSheet). The browser's own (prefs), or the app's player's (told
  // to the app, as Level the volume).
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import { HOLD, POINT, POINT_FWD, prefs, RAMP_MS, startsOf, TAPE, type StartStop } from '../prefs.svelte';
  import SettingRow from '../SettingRow.svelte';
  import { holdWind, pointWind } from '../startStop';
  import Group from './Group.svelte';
  import HoldGraph from './HoldGraph.svelte';
  import Item from './Item.svelte';
  import { resetWithUndo } from './OrderReset.svelte';
  import WindGraph from './WindGraph.svelte';

  // Each choice a row showing it, opening a sheet of its options (the owner, 2026-10-27: no pills here, they took two
  // rows on a phone).
  type Option = { value: string; label: string; line: string };
  const MODES: Option[] = [
    { value: 'off', label: 'Off', line: 'Play and pause at once' },
    { value: 'vinyl', label: 'Vinyl', line: 'As a turntable: the speed, the pitch following' },
    { value: 'fade', label: 'Fade', line: 'Fading in and out' },
  ];
  const EDGES: Option[] = [
    { value: 'stop', label: 'Stop', line: "At the track's start or end" },
    { value: 'queue', label: 'Continue', line: 'On into the track before or after it in the queue' },
  ];
  const ANIMATION: Option[] = [
    { value: 'off', label: 'Off', line: 'The info row' },
    { value: 'effects', label: 'Effects', line: 'While play or pause ramps or the tape winds' },
    { value: 'always', label: 'Always', line: 'In place of the info row' },
  ];
  const CHOICES: Record<'effect' | 'edges' | 'animation', { title: string; options: Option[]; value: () => string; set: (v: string) => void }> = {
    effect: { title: 'Effect', options: MODES, value: () => ss.mode, set: (v) => setSS({ mode: v as StartStop['mode'] }) },
    edges: { title: 'Track boundary', options: EDGES, value: () => ss.edges, set: (v) => setSS({ edges: v as StartStop['edges'] }) },
    animation: { title: 'Animation', options: ANIMATION, value: () => prefs.tapeDeck, set: (v) => (prefs.tapeDeck = v as typeof prefs.tapeDeck) },
  };
  let choosing = $state<keyof typeof CHOICES | null>(null);
  const chosen = (k: keyof typeof CHOICES) => CHOICES[k].options.find((o) => o.value === CHOICES[k].value())?.label ?? '';
  /** A motion's row, short (its name never wraps): seconds a minute and its limit, or a hold's speeds. */
  const num = (v: number) => v.toFixed(2).replace(/\.?0+$/, '');
  const pointShort = (p: { ratio: number; speed: number }) => `${num(p.ratio)} s · ${num(p.speed)}×`;
  const holdShort = (h: { start: number; top: number }) => `${num(h.start)}→${num(h.top)}×`;

  const ss = $derived(inApp ? appInfo.startStop : prefs.startStop);
  function setSS(change: Partial<StartStop>) {
    const next = { ...ss, ...change };
    if (inApp) {
      appInfo.startStop = next;
      setDevice({ startStop: next });
    } else prefs.startStop = next;
  }

  const sec = (ms: number) => `${(ms / 1000).toFixed(2).replace(/\.?0+$/, '')} s`;
  const times = (v: number) => `${v}×`;
  const perMin = (v: number) => `${v.toFixed(2).replace(/\.?0+$/, '')} s a minute`;
  const db = (v: number) => (v === 0 ? 'as played' : `${v} dB`);
  const doubling = (v: number) => `×2 every ${v.toFixed(2).replace(/\.?0+$/, '')} s`;

  /** A setting of a group (its key in it), with its range and how it reads. */
  type Tuning = { key: string; label: string; range: { min: number; max: number; step: number }; text: (v: number) => string };
  const TAPE_TUNINGS: Tuning[] = [
    { key: 'attackMs', label: 'Spin-up', range: TAPE.attackMs, text: sec },
    { key: 'brakeMs', label: 'Brake', range: TAPE.brakeMs, text: sec },
    { key: 'gainDb', label: 'Wind volume', range: TAPE.gainDb, text: db },
    { key: 'doublingS', label: 'Acceleration', range: TAPE.doublingS, text: doubling },
    { key: 'pieceMs', label: 'Piece length', range: TAPE.pieceMs, text: sec },
  ];
  const pointTunings = (fwd: boolean): Tuning[] => [
    { key: 'ratio', label: 'Duration', range: fwd ? POINT_FWD.ratio : POINT.ratio, text: perMin },
    { key: 'speed', label: 'Speed limit', range: POINT.speed, text: times },
  ];
  const HOLD_TUNINGS: Tuning[] = [
    { key: 'start', label: 'Starting speed', range: HOLD.start, text: times },
    { key: 'top', label: 'Top speed', range: HOLD.top, text: times },
  ];
  /** The sheets, each a group of the settings: the wind-up, the tape's mechanics, a motion. */
  type Part = 'tape' | 'back' | 'fwd' | 'holdBack' | 'holdFwd';
  const PARTS: Record<Part, { title: string; tunings: Tuning[]; starts: Record<string, number> }> = {
    tape: { title: 'The tape', tunings: TAPE_TUNINGS, starts: startsOf(TAPE) },
    back: { title: 'Back to a point', tunings: pointTunings(false), starts: startsOf(POINT) },
    fwd: { title: 'Forward to a point', tunings: pointTunings(true), starts: startsOf(POINT_FWD) },
    holdBack: { title: 'Holding back', tunings: HOLD_TUNINGS, starts: startsOf(HOLD) },
    holdFwd: { title: 'Holding forward', tunings: HOLD_TUNINGS, starts: startsOf(HOLD) },
  };
  let open = $state<Part | 'start' | 'stop' | null>(null);
  /** A slider being dragged: already in its sheet's graph. */
  let drag = $state<{ key: string; v: number } | null>(null);
  /** The settings as the open sheet shows them, a drag included. */
  const shown = $derived.by((): StartStop => {
    if (!drag || !open || open === 'start' || open === 'stop') return ss;
    return { ...ss, [open]: { ...ss[open], [drag.key]: drag.v } };
  });
  const valueOf = (part: Part, key: string) => (shown[part] as unknown as Record<string, number>)[key];
  const setPart = (part: Part, change: Record<string, number>) => setSS({ [part]: { ...ss[part], ...change } } as Partial<StartStop>);
  /** A sheet's settings back to their defaults, as every reset is (its header's reset, dimmed at them; Undo in the toast). */
  const atStarts = (part: Part) => Object.entries(PARTS[part].starts).every(([k, v]) => valueOf(part, k) === v);
  function resetPart(part: Part) {
    const was = { ...(ss[part] as unknown as Record<string, number>) };
    resetWithUndo(PARTS[part].title, () => {
      setPart(part, PARTS[part].starts);
      return () => setPart(part, was);
    });
  }
</script>

{#snippet slider(t: Tuning, value: number, set: (v: number) => void)}
  <SettingRow label={t.label}>
    <input
      type="range"
      use:panSafe
      min={t.range.min}
      max={t.range.max}
      step={t.range.step}
      {value}
      aria-label={t.label}
      oninput={(e) => (drag = { key: t.key, v: Number(e.currentTarget.value) })}
      onchange={(e) => {
        set(Number(e.currentTarget.value));
        drag = null;
      }}
    />
    <output>{t.text(value)}</output>
  </SettingRow>
{/snippet}

<Group title="Start and stop" more="Play and pause as a turntable (Vinyl: the speed, the pitch following) or fading in and out (Fade), over their own times: Start as play winds up, Stop as pause winds down. A track running into the next stays seamless.">
  <Item name="Effect" experimental value={chosen('effect')} onclick={() => (choosing = 'effect')} />
  {#if ss.mode !== 'off'}
    <Item name="Start" value={sec(ss.ms)} onclick={() => (open = 'start')} />
    <Item name="Stop" value={sec(ss.stopMs)} onclick={() => (open = 'stop')} />
    <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
  {/if}
</Group>

<Group title="Winding">
  <Item
    name="Tape winding"
    experimental
    more="Previous and Next wind the tape: a tap back to the track's start or on to its end, a hold for as long as you hold, ever faster; then it plays on. Each motion has its own speed."
    toggle={{ checked: ss.rewind, onchange: (on) => setSS({ rewind: on }) }}
  />
  {#if ss.rewind}
    <Item name="Back to a point" line="Previous, a seek back" value={pointShort(ss.back)} onclick={() => (open = 'back')} />
    <Item name="Forward to a point" line="Next, a seek forward" value={pointShort(ss.fwd)} onclick={() => (open = 'fwd')} />
    <Item name="Holding back" value={holdShort(ss.holdBack)} onclick={() => (open = 'holdBack')} />
    <Item name="Holding forward" value={holdShort(ss.holdFwd)} onclick={() => (open = 'holdFwd')} />
    <Item
      name="Seeking winds the tape"
      more="A seek, on the waveform or the clock, winds there instead of jumping, taking as long as a wind to a point would."
      toggle={{ checked: ss.seekWinds, onchange: (on) => setSS({ seekWinds: on }) }}
    />
    <Item name="Track boundary" value={chosen('edges')} onclick={() => (choosing = 'edges')} />
  {/if}
</Group>

<!-- The tape's mechanics, every motion's alike (and the deck's direction knob's), only while it winds (the owner,
     2026-10-27: they do nothing otherwise). -->
{#if ss.rewind}
  <Group title="The tape" more="How the tape moves, in every motion alike: how fast it gets going and how it brakes, how loud it is while it winds, how quickly a hold gets faster, and the pieces a long wind to a point is cut into.">
    {#each TAPE_TUNINGS as t (t.key)}<Item name={t.label} value={t.text(ss.tape[t.key as keyof typeof ss.tape])} onclick={() => (open = 'tape')} />{/each}
  </Group>
{/if}

<!-- This page's showing (prefs, never the app's): the player's row under the title a tape deck, while an effect runs or
     always (the owner, 2026-10-27; the section "Player", its row "Animation", 2026-10-28). -->
<Group title="Player">
  <Item
    name="Animation"
    more="The player shows a tape deck where its info row is: its reels turn at the playing speed, slowing for a pause and racing back for a rewind; its head lights on each bar's first beat. During effects: while play or pause ramps or the tape winds, then the row again."
    value={chosen('animation')}
    onclick={() => (choosing = 'animation')}
  />
</Group>

{#if choosing}
  {@const c = CHOICES[choosing]}
  <MenuSheet label={c.title} title={c.title} onclose={() => (choosing = null)}>
    {#each c.options as o (o.value)}<MenuEntry
        label={o.label}
        sub={o.line}
        checked={c.value() === o.value}
        plus={false}
        onclick={() => (c.set(o.value), (choosing = null))}
      />{/each}
  </MenuSheet>
{/if}
{#if open === 'start' || open === 'stop'}
  {@const key = open === 'start' ? 'ms' : 'stopMs'}
  {@const label = open === 'start' ? 'Start' : 'Stop'}
  <MenuSheet {label} title={label} chips={[MODES.find((m) => m.value === ss.mode)?.label ?? '']} onclose={() => ((open = null), (drag = null))}>
    <div class="sheet">
      {@render slider({ key, label, range: { ...RAMP_MS, step: 100 }, text: sec }, drag?.key === key ? drag.v : ss[key], (v) => setSS({ [key]: v }))}
    </div>
  </MenuSheet>
{:else if open}
  {@const part = open}
  {@const p = PARTS[part]}
  <MenuSheet
    label={p.title}
    title={p.title}
    end={{ icon: 'reset', label: `Reset ${p.title.toLowerCase()}`, disabled: atStarts(part), onclick: () => resetPart(part) }}
    onclose={() => ((open = null), (drag = null))}
  >
    <div class="sheet">
      {#if part === 'back' || part === 'fwd'}<WindGraph wind={pointWind(shown, part === 'fwd' ? 1 : -1)} dir={part} />{/if}
      {#if part === 'holdBack' || part === 'holdFwd'}<HoldGraph hold={holdWind(shown, part === 'holdFwd' ? 1 : -1)} />{/if}
      {#each p.tunings as t (t.key)}{@render slider(t, valueOf(part, t.key), (v) => setPart(part, { [t.key]: v }))}{/each}
    </div>
  </MenuSheet>
{/if}

<style>
  .sheet {
    padding: 0 16px 16px;
  }

  /* Each slider as wide as the sheet (the owner's iPhone, 2026-10-27: a short one under its name). */
  .sheet :global(.control) {
    flex: 1 1 14em;
  }

  .sheet input {
    flex: 1;
    width: auto;
    min-width: 0;
    accent-color: var(--accent);
  }

  .sheet output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
