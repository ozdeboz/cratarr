<script lang="ts" module>
  // An album not in the library, found by a search (Find more music's MusicBrainz, the global search's missing albums;
  // decision 0072): opened, it is looked for on Bandcamp and shown in the panel to hear; not there, its menu: Want it,
  // or look on MusicBrainz or YouTube. One place for both, so they act the same.
  export type Elsewhere = { release_group: string; title: string; artist: string; year: number | null; cover: string | null; have?: boolean; bandcamp?: string | null };
</script>

<script lang="ts">
  import { findOnBandcamp, wantAlbum } from './api';
  import { albumIdOf, BANDCAMP } from './bandcampPreview';
  import { bandcampOf, rememberBandcamp } from './bandcampFound.svelte';
  import LinkSheet from './LinkSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { albumSubject, viewOver } from './panel.svelte';
  import { toast } from './toast.svelte';

  /** Being looked for on Bandcamp (its row shows it), what was asked for, and the menu's album. */
  let {
    finding = $bindable(null),
    asked = $bindable([]),
  }: { finding?: string | null; asked?: string[] } = $props();
  let menuFor = $state<Elsewhere | null>(null);
  let link = $state<{ url: string; name: string } | null>(null);

  /** Opens it: on Bandcamp in the panel (a page known at once, lib/bandcampFound), else its menu. */
  export async function open(a: Elsewhere) {
    const known = bandcampOf(a.release_group, a.bandcamp);
    if (known) return viewOver(albumSubject(BANDCAMP, albumIdOf(known)));
    finding = a.release_group;
    try {
      const { page } = await findOnBandcamp(a.artist, a.title, a.release_group);
      if (page) {
        rememberBandcamp(a.release_group, page);
        viewOver(albumSubject(BANDCAMP, albumIdOf(page)));
      } else menuFor = a;
    } catch {
      menuFor = a;
    } finally {
      finding = null;
    }
  }
  async function want(a: Elsewhere) {
    menuFor = null;
    try {
      const { request } = await wantAlbum({ release_group: a.release_group, artist: a.artist, album: a.title, year: a.year, cover: a.cover });
      asked = [...asked, a.release_group];
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      toast.fail(e, 'Could not ask for it');
    }
  }
</script>

{#if menuFor}
  {@const a = menuFor}
  <MenuSheet label={a.title} title={a.title} images={a.cover ? [a.cover] : []} chips={[a.artist, ...(a.year ? [String(a.year)] : [])]} copy={`${a.artist} – ${a.title}`} onclose={() => (menuFor = null)}>
    <MenuCards>
      <MenuGroup label="Discover">
        {#if a.have}
          <MenuEntry icon="disc" label="In your library" disabled onclick={() => {}} />
        {:else if asked.includes(a.release_group)}
          <MenuEntry icon="inbox" label="Requested" onclick={() => ((menuFor = null), (location.hash = '#/requests'))} />
        {:else}
          <MenuEntry icon="circlePlus" label="Want" onclick={() => want(a)} />
        {/if}
      </MenuGroup>
      <MenuGroup>
        <MenuEntry icon="externalLink" label="MusicBrainz" onclick={() => ((link = { url: `https://musicbrainz.org/release-group/${a.release_group}`, name: 'MusicBrainz' }), (menuFor = null))} />
        <MenuEntry icon="search" label="Search YouTube" onclick={() => ((link = { url: `https://www.youtube.com/results?search_query=${encodeURIComponent(`${a.artist} ${a.title}`)}`, name: 'YouTube' }), (menuFor = null))} />
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
{#if link}{@const l = link}<LinkSheet url={l.url} name={l.name} onclose={() => (link = null)} />{/if}
