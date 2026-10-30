<script lang="ts">
  // The tape (the owner, 2026-10-27: one page, in order, not "Start and stop" only): Start and stop's effect; winding's
  // six motions, each on its own with one setting, its lowest Off (the owner, 2026-10-28: back to start, seek back, hold
  // back, and the same forward); the tape's own mechanics, every motion's alike; the player's animation. The same groups
  // in This device (lib/settings/DevicePart, its Tape row) and in the sheet a long press on the play button opens
  // (lib/StartStopSheet). The browser's own (prefs), or the app's player's (told to the app, as Level the volume).
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { setStartStopNow, startStopNow } from '../device.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { panSafe } from '../panSafe';
  import { MOTION, prefs, RAMP_MS, startsOf, TAPE, type Motion, type StartStop } from '../prefs.svelte';
  import SettingRow from '../SettingRow.svelte';
  import { RATE_TOP } from '../rates';
  import { HOLD_TOP, holdWind, pointWind, type PointMotion } from '../startStop';
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

  /** The six motions, in pairs, back then forward (the owner, 2026-10-28): to the start and the end, the seeks, the
   *  holds. */
  const MOTIONS: { m: Motion; name: string; line: string }[] = [
    { m: 'toStart', name: 'Back to start', line: 'Previous' },
    { m: 'toEnd', name: 'Forward to end', line: 'Next' },
    { m: 'seekBack', name: 'Seek back', line: 'The waveform, a swipe on the cover' },
    { m: 'seekFwd', name: 'Seek forward', line: 'The waveform, a swipe on the cover' },
    { m: 'holdBack', name: 'Holding back', line: 'Previous, held' },
    { m: 'holdFwd', name: 'Holding forward', line: 'Next, held' },
  ];
  const isHold = (m: Motion) => m === 'holdBack' || m === 'holdFwd';
  const backwards = (m: Motion) => m === 'toStart' || m === 'seekBack' || m === 'holdBack';

  const ss = $derived(startStopNow());
  const setSS = setStartStopNow;

  // Every value with as many decimals as its step has, always (the owner, 2026-10-28: 0.25 then 0.3 moved the slider's
  // readout while it was held).
  const places = (step: number) => (String(step).split('.')[1] ?? '').length;
  /** Seconds from ms, at `step` ms. */
  const sec = (step: number) => (ms: number) => `${(ms / 1000).toFixed(places(step / 1000))} s`;
  const db = (v: number) => (v === 0 ? 'as played' : `${v} dB`);
  /** A motion's value: a wind to a point's seconds a minute wound, a hold's seconds to its top speed; its lowest Off. */
  const motionText = (m: Motion, long: boolean) => (v: number) => {
    if (v === 0) return 'Off';
    const n = v.toFixed(places(MOTION[m].step));
    return isHold(m) ? `${n} s` : long ? `${n} s a minute` : `${n} s/min`;
  };
  /** The readout's width: its widest value, so it never moves while the slider is held. */
  const widest = (t: Tuning) =>
    Math.max(...[t.range.min, t.range.min + t.range.step, t.range.max].map((v) => t.text(v).length));

  /** A setting of a group (its key in it), with its range and how it reads. */
  type Tuning = { key: string; label: string; range: { min: number; max: number; step: number }; text: (v: number) => string };
  const TAPE_TUNINGS: Tuning[] = [
    { key: 'attackMs', label: 'Spin-up', range: TAPE.attackMs, text: sec(TAPE.attackMs.step) },
    { key: 'brakeMs', label: 'Brake', range: TAPE.brakeMs, text: sec(TAPE.brakeMs.step) },
    { key: 'gainDb', label: 'Wind volume', range: TAPE.gainDb, text: db },
  ];
  /** The sheets: the tape's mechanics, or one motion (its one setting). */
  type Part = 'tape' | Motion;
  const titleOf = (p: Part) => (p === 'tape' ? 'The tape' : MOTIONS.find((x) => x.m === p)!.name);
  const tuningsOf = (p: Part): Tuning[] =>
    p === 'tape' ? TAPE_TUNINGS : [{ key: p, label: isHold(p) ? 'Time to top speed' : 'Duration', range: MOTION[p], text: motionText(p, true) }];
  const startsOfPart = (p: Part): Record<string, number> => (p === 'tape' ? startsOf(TAPE) : { [p]: MOTION[p].start });
  /** A part's settings in `s`, and `s` with some changed. */
  const groupOf = (s: StartStop, p: Part): Record<string, number> => (p === 'tape' ? s.tape : s.motions);
  const changed = (s: StartStop, p: Part, change: Record<string, number>): StartStop =>
    p === 'tape' ? { ...s, tape: { ...s.tape, ...change } } : { ...s, motions: { ...s.motions, ...change } };

  let open = $state<Part | 'start' | 'stop' | null>(null);
  /** A slider being dragged: already in its sheet's graph. */
  let drag = $state<{ key: string; v: number } | null>(null);
  /** The settings as the open sheet shows them, a drag included. */
  const shown = $derived.by((): StartStop => {
    if (!drag || !open || open === 'start' || open === 'stop') return ss;
    return changed(ss, open, { [drag.key]: drag.v });
  });
  const valueOf = (part: Part, key: string) => groupOf(shown, part)[key];
  const setPart = (part: Part, change: Record<string, number>) => setSS(changed(ss, part, change));
  /** A sheet's settings back to their defaults, as every reset is (its header's reset, dimmed at them; Undo in the toast). */
  const atStarts = (part: Part) => Object.entries(startsOfPart(part)).every(([k, v]) => valueOf(part, k) === v);
  function resetPart(part: Part) {
    const was = Object.fromEntries(Object.keys(startsOfPart(part)).map((k) => [k, groupOf(ss, part)[k]]));
    resetWithUndo(titleOf(part), () => {
      setPart(part, startsOfPart(part));
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
    <output style:min-width="{widest(t)}ch">{t.text(value)}</output>
  </SettingRow>
{/snippet}

<Group title="Start and stop" more="Play and pause as a turntable (Vinyl: the speed, the pitch following) or fading in and out (Fade), over their own times: Start as play winds up, Stop as pause winds down. A track running into the next stays seamless.">
  <Item name="Effect" experimental value={chosen('effect')} onclick={() => (choosing = 'effect')} />
  {#if ss.mode !== 'off'}
    <Item name="Start" value={sec(100)(ss.ms)} onclick={() => (open = 'start')} />
    <Item name="Stop" value={sec(100)(ss.stopMs)} onclick={() => (open = 'stop')} />
    <Item name="Also for next and previous" toggle={{ checked: ss.skips, onchange: (on) => setSS({ skips: on }) }} />
  {/if}
</Group>

<Group title="Winding">
  <Item
    name="Tape winding"
    experimental
    more="Previous, Next and a seek wind the tape: back to the track's start or on to its end, to the place sought, or for as long as you hold, ever faster; then it plays on. Each motion has its own setting; at its lowest it is off, and goes the plain way."
    toggle={{ checked: ss.rewind, onchange: (on) => setSS({ rewind: on }) }}
  />
  {#if ss.rewind}
    {#each MOTIONS as x (x.m)}<Item name={x.name} line={x.line} value={motionText(x.m, false)(ss.motions[x.m])} onclick={() => (open = x.m)} />{/each}
    <Item name="Track boundary" value={chosen('edges')} onclick={() => (choosing = 'edges')} />
  {/if}
</Group>

<!-- The tape's mechanics, every motion's alike (and the deck's direction knob's), only while it winds (the owner,
     2026-10-27: they do nothing otherwise). -->
{#if ss.rewind}
  <Group title="The tape" more="How the tape moves, in every motion alike: how fast it gets going and how it brakes, and how loud it is while it winds.">
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
      {@render slider({ key, label, range: { ...RAMP_MS, step: 100 }, text: sec(100) }, drag?.key === key ? drag.v : ss[key], (v) => setSS({ [key]: v }))}
    </div>
  </MenuSheet>
{:else if open}
  {@const part = open}
  {@const title = titleOf(part)}
  <MenuSheet
    label={title}
    {title}
    end={{ icon: 'reset', label: `Reset ${title.toLowerCase()}`, disabled: atStarts(part), onclick: () => resetPart(part) }}
    onclose={() => ((open = null), (drag = null))}
  >
    <div class="sheet">
      {#if part !== 'tape'}
        <!-- Off: the motion as it would be at its default, dimmed (the owner, 2026-10-28: dimmed, not gone). -->
        {@const off = shown.motions[part] === 0}
        {@const drawn = off ? changed(shown, part, { [part]: MOTION[part].start }) : shown}
        <div class="graph" class:off>
          {#if isHold(part)}<HoldGraph hold={holdWind(drawn, backwards(part) ? -1 : 1, inApp ? HOLD_TOP : Math.min(HOLD_TOP, RATE_TOP))} />
          {:else}<WindGraph wind={pointWind(drawn, part as PointMotion)} dir={backwards(part) ? 'back' : 'fwd'} />{/if}
        </div>
      {/if}
      {#each tuningsOf(part) as t (t.key)}{@render slider(t, valueOf(part, t.key), (v) => setPart(part, { [t.key]: v }))}{/each}
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

  .graph.off {
    opacity: 0.35;
  }

  .sheet output {
    min-width: 4ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
