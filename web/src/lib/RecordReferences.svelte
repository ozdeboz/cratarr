<script lang="ts">
  // A record's references (0.14; the owner, 2026-11-02: a row per source): MusicBrainz, Discogs, Bandcamp and AcoustID,
  // each agreeing, differing, found (nothing to compare yet) or missing, with what it says; the one that counts marked,
  // Use this on another, Confirm on a guessed one, its page where it has one. Where sources give lengths, they stand side
  // by side with the record's (the owner, 2026-11-02: disagreeing sources side by side). Paste a link (its source then
  // counts) and Checked by hand under them. Health's References item (records none speaks for) and a record's own Health
  // line open it.
  import { albumView, getReferences, setReferences, type PlaylistRow, type Reference, type References, type ReferenceSource } from './api';
  import { clockText } from './remote.svelte';
  import { loadHealth } from './healthReport.svelte';
  import { me } from './me.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import ValueSheet from './settings/ValueSheet.svelte';
  import { toast } from './toast.svelte';

  let { album }: { album: string } = $props();

  let refs = $state<References | null>(null);
  let failed = $state<string | null>(null);
  let again = $state(0);
  $effect(() => {
    void again;
    let gone = false;
    getReferences(album).then(
      (r) => !gone && ((refs = r), (failed = null)),
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => (gone = true);
  });
  // The record's own lengths, beside the sources' where any gives them.
  const timed = $derived(refs?.sources.filter((x) => x.lengths?.some((ms) => ms > 0)) ?? []);
  let rows = $state<PlaylistRow[] | null>(null);
  $effect(() => {
    if (!timed.length || rows) return;
    let gone = false;
    albumView('cratarr', album).then((v) => !gone && (rows = v.rows), () => {});
    return () => (gone = true);
  });
  const longest = $derived(Math.max(rows?.length ?? 0, ...timed.map((x) => x.lengths?.length ?? 0)));
  const time = (ms: number | null | undefined) => (ms ? clockText(ms / 1000) : '–');
  /** A source's length off the record's by more than five seconds (the server's own rule for "differs"). */
  const off = (ms: number | undefined, i: number) => {
    const own = rows?.[i]?.duration_secs;
    return !!ms && !!own && Math.abs(ms / 1000 - own) > 5;
  };

  const NAME: Record<ReferenceSource, string> = { musicbrainz: 'MusicBrainz', discogs: 'Discogs', bandcamp: 'Bandcamp', acoustid: 'AcoustID' };
  const LEAD: Record<Reference['state'], string> = { agrees: 'on', differs: 'warn', found: 'off', missing: '' };
  /** What it says besides its words: tracks (where its words do not count them), year, type; guessed until confirmed. */
  const lineOf = (r: Reference) =>
    [r.said, r.tracks && !/\btracks?\b/.test(r.said) ? `${r.tracks} ${r.tracks === 1 ? 'track' : 'tracks'}` : '', r.year ? String(r.year) : '', r.type ?? '', r.guessed && !r.confirmed ? 'Guessed' : '']
      .filter(Boolean)
      .join(' · ');

  let busy = $state<string | null>(null);
  async function choose(what: string, choice: Parameters<typeof setReferences>[1]) {
    if (busy) return;
    busy = what;
    try {
      refs = await setReferences(album, choice);
      loadHealth();
    } catch (e) {
      toast.fail(e, "The reference wasn't changed");
    } finally {
      busy = null;
    }
  }
  let pasting = $state(false);
</script>

{#if failed && !refs}
  <Notice text={failed} onretry={() => again++} />
{:else if !refs}
  <PageLoading />
{:else}
  {@const r = refs}
  <Group>
    {#each r.sources as x (x.source)}
      {@const counts = r.counting === x.source}
      <Item lead={LEAD[x.state]} name={NAME[x.source] ?? x.source} line={lineOf(x)} tone={x.state === 'differs' ? 'warn' : null} value={counts ? 'Counts' : null} busy={busy === x.source} wrap>
        <!-- The row's own buttons, as every settings row's (lib/settings/Item). -->
        {#if x.url}{@const url = x.url}<button onclick={() => window.open(url, '_blank', 'noopener')}>Page</button>{/if}
        {#if me.admin && x.guessed && !x.confirmed && x.state !== 'missing'}<button disabled={!!busy} onclick={() => void choose(x.source, { confirm: x.source })}>Confirm</button>{/if}
        {#if me.admin && !counts && x.state !== 'missing'}<button disabled={!!busy} onclick={() => void choose(x.source, { counting: x.source })}>Use this</button>{/if}
      </Item>
    {/each}
  </Group>
  {#if timed.length}
    <Group title="Lengths">
      <div class="lengths" style:--cols={timed.length + 1}>
        <span class="head">#</span><span class="head">Record</span>{#each timed as x (x.source)}<span class="head">{NAME[x.source] ?? x.source}</span>{/each}
        {#each Array.from({ length: longest }, (_, i) => i) as i (i)}
          <span class="num">{i + 1}</span><span>{time(rows?.[i]?.duration_secs ? rows[i].duration_secs! * 1000 : null)}</span>
          {#each timed as x (x.source)}<span class:off={off(x.lengths?.[i], i)}>{time(x.lengths?.[i])}</span>{/each}
        {/each}
      </div>
    </Group>
  {/if}
  {#if me.admin}
    <Group>
      <Item name="Paste a link" line="MusicBrainz, Discogs or Bandcamp" busy={busy === 'link'} onclick={() => (pasting = true)} />
      <Item name="Checked by hand" line="The record is its own reference; its type and year as set" toggle={{ checked: r.by_hand, onchange: (on) => void choose('hand', { by_hand: on }) }} />
    </Group>
  {/if}
  {#if pasting}
    <ValueSheet title="Paste a link" type="url" value="" placeholder="https://" onsave={(v) => choose('link', { link: v.trim() })} onclose={() => (pasting = false)} />
  {/if}
{/if}

<style>
  /* The record's lengths and each source's, track by track; one off by more than five seconds marked. */
  .lengths {
    display: grid;
    grid-template-columns: 2em repeat(var(--cols), minmax(0, 1fr));
    gap: 4px 10px;
    padding: 10px 14px;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }

  .head {
    color: var(--muted);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .num {
    color: var(--muted);
  }

  .off {
    color: var(--warn-ink);
    font-weight: 600;
  }
</style>
