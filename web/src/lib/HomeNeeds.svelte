<script lang="ts">
  // Home's Needs you (the owner, 2026-10-23): what waits on the owner, each a row that opens where it is dealt with:
  // upgrades and duplicates waiting in an Incoming library (the Library, with its switch on), and how far AudioMuse
  // has heard the library while it has not heard all of it. Nothing waiting: the tile does not show. An
  // administrator's only: what waits is theirs to deal with.
  import { getAlbums, getAudioMuseProgress } from './api';
  import Icon, { type IconName } from './Icon.svelte';
  import { setSwitch } from './listFilter.svelte';
  import { me } from './me.svelte';
  import SectionHead from './SectionHead.svelte';

  let waiting = $state({ upgrades: 0, duplicates: 0 });
  let heard = $state<{ heard: number; tracks: number } | null>(null);

  $effect(() => {
    if (!me.admin) return;
    let gone = false;
    getAlbums().then(
      (v) => {
        if (gone) return;
        const all = v.sources.flatMap((s) => Object.values(s.verdicts));
        waiting = { upgrades: all.filter((x) => x.kind === 'upgrade').length, duplicates: all.filter((x) => x.kind === 'duplicate').length };
      },
      () => {},
    );
    getAudioMuseProgress().then(
      (p) => !gone && (heard = p),
      () => {},
    );
    return () => (gone = true);
  });

  /** The Library with one of its switches on, the others as they were. */
  function open(key: 'upgrades' | 'duplicates') {
    setSwitch('library', key, true);
    location.hash = '#/library';
  }

  const plural = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
  const rows = $derived.by(() => {
    const out: { icon: IconName; text: string; open?: () => void }[] = [];
    if (waiting.upgrades) out.push({ icon: 'arrowUp', text: plural(waiting.upgrades, 'upgrade waiting', 'upgrades waiting'), open: () => open('upgrades') });
    if (waiting.duplicates)
      out.push({ icon: 'copyCheck', text: plural(waiting.duplicates, 'duplicate waiting', 'duplicates waiting'), open: () => open('duplicates') });
    if (heard && heard.tracks > 0 && heard.heard < heard.tracks)
      out.push({ icon: 'music', text: `AudioMuse has heard ${heard.heard.toLocaleString()} of ${heard.tracks.toLocaleString()}` });
    return out;
  });
  const share = $derived(heard && heard.tracks ? heard.heard / heard.tracks : 0);
</script>

{#if me.admin && rows.length}
  <section aria-label="Needs you">
    <!-- Its arrow: the Health page, where all of it is (decision 0063). -->
    <SectionHead title="Needs you" onmore={() => (location.hash = '#/health')} />
    <div class="needs">
      {#each rows as r (r.text)}
        {#if r.open}
          <button class="row" onclick={r.open}><Icon name={r.icon} size={22} /><span>{r.text}</span><Icon name="chevronRight" size={20} /></button>
        {:else}
          <div class="row">
            <Icon name={r.icon} size={22} /><span>{r.text}<i class="bar" style:--share={share} aria-hidden="true"></i></span>
          </div>
        {/if}
      {/each}
    </div>
  </section>
{/if}

<style>
  /* As Home's other lists: 16 px in, 8 px apart. */
  .needs {
    display: grid;
    gap: 8px;
    margin: 4px 16px 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 10%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
  }

  button.row {
    cursor: pointer;
  }

  .row :global(svg:first-child) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .row :global(svg:last-child:not(:first-child)) {
    flex: none;
    color: var(--muted);
  }

  .row span {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 6px;
  }

  /* AudioMuse's share heard, as a thin line under its words. */
  .bar {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--art-accent, var(--accent)) calc(var(--share) * 100%),
      color-mix(in srgb, var(--text) 15%, transparent) calc(var(--share) * 100%)
    );
  }
</style>
