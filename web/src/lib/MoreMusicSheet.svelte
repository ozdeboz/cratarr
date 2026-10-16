<script lang="ts" module>
  import { local, type AutoplayMode, type RadioBy } from './localPlayer.svelte';

  export const AUTOPLAY_NAMES: Record<AutoplayMode, string> = { similar: 'Similar', artist: 'Same artist', era: 'Same era', random: 'Random' };
  export const RADIO_NAMES: Record<RadioBy, string> = { track: 'Similar to the track', artist: 'Same artist', era: 'Same era' };

  /** What is on, part by part, each with its icon (the icon follows what is on, the owner said): the queue's pill. */
  export function moreMusicParts(): { icon: 'radio' | 'infinity'; text: string }[] {
    const parts: { icon: 'radio' | 'infinity'; text: string }[] = [];
    if (local.radio) parts.push({ icon: 'radio', text: local.radioBy === 'track' ? 'Radio' : `Radio, ${RADIO_NAMES[local.radioBy].toLowerCase()}` });
    if (local.autoplay) parts.push({ icon: 'infinity', text: `${AUTOPLAY_NAMES[local.autoplay.mode]}${local.autoplay.albums ? ', albums' : ''}${local.autoplay.restart ? ', restart' : ''}` });
    return parts;
  }

  /** What is on, in a line: the sheet's chip ("Radio · ∞ Similar", "Off"). */
  export function moreMusicLine() {
    const parts = [local.radio ? (local.radioBy === 'track' ? 'Radio' : `Radio, ${RADIO_NAMES[local.radioBy].toLowerCase()}`) : null, local.autoplay ? `∞ ${AUTOPLAY_NAMES[local.autoplay.mode]}${local.autoplay.albums ? ', albums' : ''}${local.autoplay.restart ? ', restart' : ''}` : null];
    return parts.filter(Boolean).join(' · ') || 'Off';
  }
</script>

<script lang="ts">
  // More music (decision 0036, amended; one sheet for both, decided 2026-10-16): what plays besides what was chosen.
  // Radio, between: tracks like each of the queue's own tracks (similar to it, its artist or its era), so many after
  // it. Autoplay, at the end: what plays when the queue runs out, then its switches (whole albums while albums were
  // playing; a playlist starting again instead), each saying what it does while it is on. Each a choice of one, as View's order (a round mark); the sheet stays
  // open while it is set. From the queue's floating pill and the player's radio button.
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import SettingRow from './SettingRow.svelte';
  import Switch from './Switch.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { RADIO_BYS, RADIO_MAX, setAutoplay, setRadio, setSkipFavourites } from './localPlayer.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const RADIO: { value: RadioBy | 'off'; label: string }[] = [{ value: 'off', label: 'Off' }, ...RADIO_BYS.map((by) => ({ value: by, label: RADIO_NAMES[by] }))];
  const radioNow = $derived<RadioBy | 'off'>(local.radio ? local.radioBy : 'off');
  // How many after each track: a slider (the owner), 1 to the player's most; the number follows the finger, and is set
  // when it lifts (each setting asks the server anew).
  let dragging = $state<number | null>(null);
  const AUTOPLAY: { value: AutoplayMode | 'stop'; label: string }[] = [
    { value: 'stop', label: 'Stop' },
    ...(Object.entries(AUTOPLAY_NAMES) as [AutoplayMode, string][]).map(([value, label]) => ({ value, label })),
  ];
  const now = $derived<AutoplayMode | 'stop'>(local.autoplay?.mode ?? 'stop');
  const albums = $derived(local.autoplay?.albums ?? false);
  const restart = $derived(local.autoplay?.restart ?? false);
</script>

<MenuSheet label="More music" icon="radio" title="More music" chips={[moreMusicLine()]} {onclose}>
  <div class="parts">
    <Card title="Radio" note="after each of your tracks" label="Radio">
      <ChoiceList label="Radio" options={RADIO} value={radioNow} onchange={(v) => (v === 'off' ? setRadio(false) : setRadio(true, { by: v }))} />
      <div class="count" class:off={!local.radio}>
        <SettingRow label="After each track">
          <input
            type="range"
            min="1"
            max={RADIO_MAX}
            step="1"
            value={dragging ?? local.radioCount}
            aria-label="Tracks after each track"
            oninput={(e) => (dragging = Number(e.currentTarget.value))}
            onchange={(e) => {
              setRadio(local.radio, { count: Number(e.currentTarget.value) });
              dragging = null;
            }}
          />
          <output>{dragging ?? local.radioCount}</output>
        </SettingRow>
      </div>
    </Card>
    <Card title="Autoplay" note="when the queue ends" label="Autoplay">
      <ChoiceList label="Autoplay" options={AUTOPLAY} value={now} onchange={(v) => setAutoplay(v)} />
    </Card>
    <div class="toggles">
      <!-- For both, and the first of the three (the owner): hearted tracks are left out of what Radio and Autoplay pick. -->
      <Switch
        card
        label="Ignore favourites"
        hint="Radio and Autoplay leave out your hearted tracks."
        hintWhenOn
        checked={local.skipFavourites}
        onchange={setSkipFavourites}
      />
      <Switch
        card
        label="Queue albums"
        hint="While albums play, whole albums instead of single tracks."
        hintWhenOn
        checked={albums}
        disabled={now === 'stop'}
        onchange={(on) => setAutoplay(now, { albums: on })}
      />
      <Switch
        card
        label="Restart playlists"
        hint="A playlist that ends starts again (shuffled if it played shuffled)."
        hintWhenOn
        checked={restart}
        disabled={now === 'stop'}
        onchange={(on) => setAutoplay(now, { restart: on })}
      />
    </div>
  </div>
</MenuSheet>

<style>
  /* Its parts are cards (lib/Card), each a choice of one (lib/ChoiceList); the switches lib/Switch's. */
  .parts {
    --card: color-mix(in srgb, var(--text) 6%, transparent);
    display: grid;
    gap: 8px;
    padding: 4px 16px 16px;
  }

  /* How many after each track, at Radio's foot: a slider, its number after it; dimmed while Radio is off. */
  .count {
    margin-top: 4px;
    border-top: 1px solid var(--border);
  }

  .count.off {
    opacity: 0.55;
  }

  .count input {
    width: 11em;
    max-width: 100%;
    accent-color: var(--art-accent, var(--accent));
  }

  .count output {
    width: 2ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .toggles {
    display: grid;
    gap: 3px;
  }
</style>
