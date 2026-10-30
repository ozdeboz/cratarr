<script lang="ts">
  // An album's About (as Symfonium's, decided 2026-10-16), at the end of its panel: its genres (a chip each, opening the
  // genre), year, when it was last played and added (date and time, as Symfonium writes them), and where it comes
  // from: the server and its library. What is not known is left out; Last played says Never when it never was. Its
  // rating (decided 2026-10-16) as five stars, the same as its menu's: a tap rates, where its server keeps ratings.
  // A library album's Discogs (0.9.8, the owner: "Yes to all"): its labels with their catalogue numbers, its credits
  // by role, its styles beside its genres, and its page among Elsewhere; asked once (Discogs keeps a pace: they come
  // in a moment the first time).
  import { albumRatings, loadAlbumRating, rateAlbum } from './albums.svelte';
  import { CRATE, GENRE, viewGenre } from './crates.svelte';
  import { longpress } from './longpress';
  import SubjectMenu from './SubjectMenu.svelte';
  import RatingStars from './RatingStars.svelte';
  import Icon from './Icon.svelte';
  import SectionHead from './SectionHead.svelte';
  import { getAlbumDiscogs, type AlbumDiscogs } from './api';

  let {
    source,
    album,
    genres,
    year,
    added,
    played,
    plays,
    provider,
    more = null,
  }: {
    source: string;
    album: string;
    genres: string[];
    year: number | null;
    added?: number;
    played?: number;
    plays?: number;
    /** The server's name, and the library's (with its dot) when known. */
    provider: { server: string; library: string | null; colour: string | null };
    /** What a page elsewhere says of it (a Bandcamp album's, 0.9.2): no rating then, which only a server keeps. */
    more?: Pick<import('./api').AlbumView, 'released' | 'label' | 'about' | 'credits' | 'tags' | 'links' | 'listeners'> | null;
  } = $props();

  $effect(() => {
    if (!more) loadAlbumRating(source, album);
  });
  const rating = $derived(albumRatings[`${source}/${album}`] ?? null);

  let discogs = $state<AlbumDiscogs | null>(null);
  $effect(() => {
    const [s, a] = [source, album];
    discogs = null;
    if (more) return;
    let gone = false;
    getAlbumDiscogs(s, a).then(
      (d) => !gone && (discogs = d),
      () => {},
    );
    return () => (gone = true);
  });
  /** Discogs' styles not among the genres already shown. */
  const styles = $derived((discogs?.styles ?? []).filter((x) => !genres.some((g) => g.toLowerCase() === x.toLowerCase())));
  /** Its credits by role, in Discogs' order: "Producer: A, B"; a part of the release says which ("Mixed By (A2): C"). */
  const credits = $derived.by(() => {
    const by = new Map<string, string[]>();
    for (const c of discogs?.credits ?? []) {
      const role = c.tracks ? `${c.role} (${c.tracks})` : c.role;
      by.set(role, [...(by.get(role) ?? []), c.name]);
    }
    return [...by].map(([role, names]) => `${role}: ${names.join(', ')}`).join('\n');
  });

  const two = (n: number) => String(n).padStart(2, '0');
  /** 2026-10-16 14:05:09, this device's time. */
  const when = (secs: number) => {
    const d = new Date(secs * 1000);
    return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
  };
  // A badge's menu (decision 0058: a chip that opens an object opens its menu on a long-press).
  let menuOf = $state<string | null>(null);
</script>

<section aria-label="About">
  <SectionHead title="About" />
  <div class="body">
  {#if genres.length}
    <h3>Genres</h3>
    <p class="chips">
      {#each genres as g (g)}<button class="chip" onclick={() => viewGenre(g)} use:longpress={() => (menuOf = CRATE + GENRE + g)}>{g}</button>{/each}
    </p>
  {/if}
  {#if styles.length}
    <h3>Styles</h3>
    <p class="chips">{#each styles as g (g)}<span class="chip">{g}</span>{/each}</p>
  {/if}
  {#if more?.tags?.length}
    <h3>Tags</h3>
    <p class="chips">{#each more.tags as t (t)}<span class="chip">{t}</span>{/each}</p>
  {/if}
  {#if more?.released}
    <h3>Released</h3>
    <p>{new Date(`${more.released}T00:00`).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}</p>
  {:else if year}
    <h3>Year</h3>
    <p class="chips"><span class="chip">{year}</span></p>
  {/if}
  {#if more?.label}
    <h3>Label</h3>
    <p>{more.label}</p>
  {:else if discogs?.labels?.length}
    <h3>{discogs.labels.length === 1 ? 'Label' : 'Labels'}</h3>
    <p class="words">{discogs.labels.map((l) => (l.catno && l.catno.toLowerCase() !== 'none' ? `${l.name} · ${l.catno}` : l.name)).join('\n')}</p>
  {/if}
  {#if more?.listeners}
    <h3>Listeners</h3>
    <p>{more.listeners.toLocaleString()} on Last.fm</p>
  {/if}
  {#if more?.about}
    <h3>About</h3>
    <p class="words">{more.about}</p>
  {/if}
  {#if more?.credits}
    <h3>Credits</h3>
    <p class="words">{more.credits}</p>
  {:else if credits}
    <h3>Credits</h3>
    <p class="words">{credits}</p>
  {/if}
  {#if more?.links?.length}
    <h3>Elsewhere</h3>
    <p class="chips">{#each more.links as l (l.url)}<a class="chip" href={l.url} target="_blank" rel="noopener noreferrer">{l.name}</a>{/each}</p>
  {:else if discogs?.url}
    <h3>Elsewhere</h3>
    <p class="chips"><a class="chip" href={discogs.url} target="_blank" rel="noopener noreferrer">Discogs</a></p>
  {/if}
  {#if rating !== null}
    <h3>Rating</h3>
    <div class="stars"><RatingStars value={rating} onrate={(n) => rateAlbum(source, album, n)} /></div>
  {/if}
  {#if played || plays === 0}
    <h3>Last played</h3>
    <p>{played ? when(played) : 'Never'}</p>
  {/if}
  {#if added}
    <h3>Date added</h3>
    <p>{when(added)}</p>
  {/if}
  <h3>Media provider</h3>
  <p class="chips">
    <span class="chip">
      <Icon name="server" size={16} />
      {provider.server}{#if provider.library}<span class="sep">·</span>{#if provider.colour}<i class="dot" style:background={provider.colour}></i>{/if}{provider.library}{/if}
    </span>
  </p>
  </div>
</section>

{#if menuOf}<SubjectMenu subject={menuOf} onclose={() => (menuOf = null)} />{/if}

<style>
  section {
    margin-top: 24px;
  }

  .body {
    padding: 0 16px;
  }

  /* As Symfonium's: the facts' names plain and muted, what they say a little in from them. */
  h3 {
    margin: 14px 0 8px;
    color: var(--muted);
    font-size: 17px;
    font-weight: 400;
  }

  p {
    margin: 0;
    padding-left: 8px;
    font-size: 15px;
    font-weight: 500;
  }

  /* Its own words (a page's description, its credits): as written, lines kept. */
  .words {
    color: var(--muted);
    font-weight: 400;
    line-height: 1.55;
    white-space: pre-line;
  }

  a.chip {
    text-decoration: none;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* Material's outlined chip, as Symfonium's. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    min-height: 32px;
    padding: 0 12px;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
  }

  button.chip {
    cursor: pointer;
  }

  /* The stars' own room makes their indent: the first one's left edge lines up with the chips'. */
  .stars {
    margin-left: -4px;
  }

  .sep {
    color: var(--muted);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
</style>
