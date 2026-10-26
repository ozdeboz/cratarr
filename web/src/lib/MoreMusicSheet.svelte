<script lang="ts" module>
  import { panSafe } from './panSafe';
  import { local, type AutoplayMode, type RadioBy } from './localPlayer.svelte';

  export const AUTOPLAY_NAMES: Record<AutoplayMode, string> = { similar: 'Similar', artist: 'Same artist', era: 'Same era', random: 'Random' };
  export const RADIO_NAMES: Record<RadioBy, string> = { track: 'Similar to the track', artist: 'Same artist', era: 'Same era' };

  /** What is on, part by part, each with its icon (the icon follows what is on, the owner said): the queue's pill. */
  export function moreMusicParts(): { icon: 'radio' | 'infinity'; text: string }[] {
    const parts: { icon: 'radio' | 'infinity'; text: string }[] = [];
    if (local.radio) parts.push({ icon: 'radio', text: local.radioBy === 'track' ? 'Radio' : `Radio, ${RADIO_NAMES[local.radioBy].toLowerCase()}` });
    if (local.autoplay) parts.push({ icon: 'infinity', text: `${AUTOPLAY_NAMES[local.autoplay.mode]}${local.autoplay.albums ? ', albums' : ''}` });
    return parts;
  }

  /** What is on, in a line: the sheet's chip ("Radio · ∞ Similar", "Off"). */
  export function moreMusicLine() {
    const parts = [local.radio ? (local.radioBy === 'track' ? 'Radio' : `Radio, ${RADIO_NAMES[local.radioBy].toLowerCase()}`) : null, local.autoplay ? `∞ ${AUTOPLAY_NAMES[local.autoplay.mode]}${local.autoplay.albums ? ', albums' : ''}` : null];
    return parts.filter(Boolean).join(' · ') || 'Off';
  }
</script>

<script lang="ts">
  // More music (decision 0036, amended; one sheet for both, decided 2026-10-16): what plays besides what was chosen.
  // Radio, between: tracks like each of the queue's own tracks (similar to it, its artist or its era), so many after
  // it. Autoplay, at the end: what plays when the queue runs out, then its switches (whole albums while albums were
  // playing), saying what it does while it is on (Restart playlists is gone, the owner 2026-10-24). Each a choice of one, as View's order (a round mark); the sheet stays
  // open while it is set. What depends on a choice shows only while that is on, beside it (the owner, 2026-10-24): how
  // many after each track in Radio's card, Autoplay's switch in its card, Ignore favourites while either plays. From
  // the queue's floating pill and the player's radio button.
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import SettingRow from './SettingRow.svelte';
  import Switch from './Switch.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import SimilarBy from './SimilarBy.svelte';
  import { RADIO_BYS, RADIO_MAX, setAutoplay, setRadio, setSkipFavourites, sourcesChanged } from './localPlayer.svelte';
  import { library, readLibrary } from './albums.svelte';
  import { libraryOn, mine, setMoreFrom, turnLibrary } from './mySettings.svelte';
  import { OWN } from './sources.svelte';

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

  // From (the owner, 2026-10-25: the sources in the sheet too, "otherwise there will be no way to control that"): the
  // account's own, as Settings' Radio and Autoplay has them; every player reads them, so a change counts at once.
  if (!library.view) readLibrary(false).catch(() => {});
  const libraries = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);
  const changedThen = (p: Promise<void>) => p.then(sourcesChanged, () => {});
</script>

<MenuSheet label="More music" icon="radio" title="More music" chips={[moreMusicLine()]} {onclose}>
  <div class="parts">
    <Card title="Radio" note="after each of your tracks" label="Radio">
      <ChoiceList label="Radio" options={RADIO} value={radioNow} onchange={(v) => (v === 'off' ? setRadio(false) : setRadio(true, { by: v }))} />
      {#if radioNow === 'track'}<SimilarBy />{/if}
      {#if local.radio}<div class="count">
        <SettingRow label="After each track">
          <input
            type="range" use:panSafe
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
      </div>{/if}
    </Card>
    <Card title="Autoplay" note="when the queue ends" label="Autoplay">
      <ChoiceList label="Autoplay" options={AUTOPLAY} value={now} onchange={(v) => setAutoplay(v)} />
      {#if now === 'similar'}<SimilarBy />{/if}
      {#if now !== 'stop'}
        <div class="switches">
          <Switch
            label="Queue albums"
            hint="While albums play, whole albums instead of single tracks."
            hintWhenOn
            checked={albums}
            onchange={(on) => setAutoplay(now, { albums: on })}
          />
        </div>
      {/if}
    </Card>
    {#if local.radio || now !== 'stop'}
      <!-- For both (the owner): hearted tracks are left out of what Radio and Autoplay pick. -->
      <Switch
        card
        label="Ignore favourites"
        hint="Radio and Autoplay leave out your hearted tracks."
        hintWhenOn
        checked={local.skipFavourites}
        onchange={setSkipFavourites}
      />
      <Card title="From" note="where they find music" label="From">
        {#each libraries as l (l.id)}
          <Switch label={l.name} checked={libraryOn(l.id)} onchange={(on) => changedThen(turnLibrary(l.id, on, libraries.map((x) => x.id)))} />
        {/each}
        <Switch
          label="Bandcamp"
          hint="Discover's suggestions that play there, as previews."
          hintWhenOn
          checked={mine.moreFrom.bandcamp}
          onchange={(on) => changedThen(setMoreFrom({ bandcamp: on }))}
        />
      </Card>
    {/if}
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

  /* How many after each track, at Radio's foot (while Radio is on): a slider, its number after it. */
  .count {
    margin-top: 4px;
    border-top: 1px solid var(--border);
  }

  /* Autoplay's switches, at its card's foot (while it plays). */
  .switches {
    margin-top: 4px;
    border-top: 1px solid var(--border);
  }

  /* The sheet's one slider, as wide as the sheet (the owner's iPhone, 2026-10-27: a short one under its name). */
  .count :global(.control) {
    flex: 1 1 14em;
  }

  .count input {
    flex: 1;
    width: auto;
    min-width: 0;
    accent-color: var(--art-accent, var(--accent));
  }

  .count output {
    width: 2ch;
    margin-left: 8px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

</style>
