<script lang="ts">
  // Global search's words read as a crate (0.12; the owner, 2026-10-30: mockup 1a, "a crate card first"): where they
  // read as two terms or more of two kinds or more (lib/quickCrate crateOfLine), the first result is the crate they
  // make: four of its covers, its sentence in small, how many tracks, Grab this crate (Crates, the sentence filled in)
  // and Play. One word, or one kind: the plain Grab a crate row instead (`plain`). Read once the search's own answer for
  // the words is in (so nothing per keystroke), that answer reused for the whole line; the crate's count and covers once
  // it reads.
  import { untrack } from 'svelte';
  import { albumCoverUrl, previewCrate, search, type PlaylistRow } from './api';
  import Cover from './Cover.svelte';
  import { globalSearch } from './globalSearch.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import { TERM_KINDS, conditionsOf, crateOfLine, nameOf, readLine, sentenceOf, type Chip, type Found } from './quickCrate';
  import { grabCrate } from './quickCrates.svelte';
  import { words } from './search';
  import { toast } from './toast.svelte';

  let { q, plain }: { q: string; plain: import('svelte').Snippet } = $props();

  /** How many of each kind a phrase's readings offer, as the crate search asks. */
  const EACH = 3;
  const fromAnswer = (r: import('./api').ServerSearch): Found => TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, EACH) ?? []);

  let chips = $state<Chip[] | null>(null);
  let preview = $state<{ source: string; total: number; rows: PlaylistRow[] } | null>(null);
  // Read when the search's answer for these words is in; the whole line from that answer, its shorter phrases asked.
  const answered = $derived(globalSearch.found?.q === q ? globalSearch.found.r : null);
  $effect(() => {
    const r = answered;
    const line = q;
    chips = null;
    preview = null;
    if (!r || words(line).length < 2) return;
    let gone = false;
    const whole = words(line).join(' ');
    const ask = (phrase: string): Promise<Found> =>
      words(phrase).join(' ') === whole ? Promise.resolve(fromAnswer(r)) : search(phrase, [...TERM_KINDS], EACH).then(fromAnswer);
    untrack(() =>
      readLine(line, ask).then(
        (reads) => {
          if (gone) return;
          const c = crateOfLine(reads);
          chips = c;
          if (!c) return;
          previewCrate(conditionsOf(c), 4, prefs.source || undefined, { without: hiddenLibraries() }).then(
            (p) => !gone && (preview = p),
            () => {},
          );
        },
        () => {},
      ),
    );
    return () => (gone = true);
  });

  const parts = $derived(chips ? sentenceOf(chips) : []);
  /** Four covers, an album's once. */
  const covers = $derived.by(() => {
    if (!preview) return [];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const r of preview.rows) {
      const key = r.album_id ?? r.id ?? '';
      if (!r.album_id || seen.has(key)) continue;
      seen.add(key);
      out.push(albumCoverUrl(preview.source, r.album_id));
      if (out.length === 4) break;
    }
    return out;
  });
  const likeAt = (i: number | undefined) => i !== undefined && !!chips?.[i]?.reading.id && chips?.[i]?.how === 'like';

  function grab() {
    if (chips) grabCrate(chips);
  }
  let starting = $state(false);
  async function play() {
    if (!chips) return;
    starting = true;
    try {
      const p = await previewCrate(conditionsOf(chips), 500, prefs.source || undefined, { without: hiddenLibraries() });
      const ids = p.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : []));
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, 'play', nameOf(chips));
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      starting = false;
    }
  }
</script>

{#if chips}
  <section class="card" aria-label="A crate of {q}">
    <span class="mosaic" aria-hidden="true">
      {#each [0, 1, 2, 3] as k (k)}<span class="tile">{#if covers[k]}<Cover src={covers[k]} />{/if}</span>{/each}
    </span>
    <span class="meta">
      <span class="sentence">{#each parts as p, k (k)}{#if !p.tight && k}{' '}{/if}{#if p.chip !== undefined}<u class:like={likeAt(p.chip)}>{p.text}</u>{:else}<i>{p.text}</i>{/if}{/each}</span>
      {#if preview}<span class="count">{plural(preview.total, 'track')}</span>{/if}
      <span class="acts">
        <button class="grab" onclick={grab}>Grab this crate</button>
        <button class="play" aria-label="Play this crate" title="Play" disabled={starting} onclick={play}><Icon name="play" size={18} /></button>
      </span>
    </span>
  </section>
{:else}
  {@render plain()}
{/if}

<style>
  /* The crate card (mockup 1a): Crates' cover mosaic and the crate search's sentence, small, on the accent's ring. */
  .card {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 12px;
    margin: 6px 0 10px;
    padding: 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    border-radius: 14px;
    background: var(--surface);
  }

  .mosaic {
    display: grid;
    grid-template-columns: 1fr 1fr;
    width: 96px;
    height: 96px;
    overflow: hidden;
    border-radius: 8px;
    background: var(--tile);
  }

  .tile {
    display: block;
    overflow: hidden;
  }

  .meta {
    display: grid;
    gap: 6px;
    align-content: start;
    min-width: 0;
  }

  .sentence {
    font-size: 15px;
    line-height: 1.45;
  }

  .sentence u {
    border-bottom: 2px solid var(--border);
    font-weight: 600;
    text-decoration: none;
  }

  .sentence u.like {
    border-bottom-color: var(--accent);
  }

  .sentence i {
    color: var(--muted);
    font-style: normal;
  }

  .count {
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .acts {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .grab {
    padding: 8px 14px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .play {
    display: grid;
    width: 36px;
    height: 36px;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    cursor: pointer;
  }
</style>
