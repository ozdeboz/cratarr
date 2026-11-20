<script lang="ts">
  // A menu's Discover group for something on Bandcamp (decision 0071, the owner 2026-10-25: "the Discover buttons need
  // their own section or group"): the Discover suggestion its album page is, Want (or Requested) and Not interested,
  // as on Discover's own menu. Not a suggestion (the owner, 2026-10-25: "When playing any Bandcamp release, its context
  // menu should offer the Want option"): Want for it (or Requested), as for any missing album: by the record that holds its
  // page, else by the release group it was found by and its page, with the album's own names (W12, lib/wantPath; a track is
  // wanted as its album, by the album's artist). Requested comes from the server: the account's requests of that record, else
  // that group, never names. The player's track menu and a Bandcamp album's.
  import { albumView, getDiscover, getRequests, notDiscover, wantAlbum, wantDiscover, type DiscoverAlbum } from './api';
  import { hrefOf } from './addresses';
  import { sayWantFailed } from './albums.svelte';
  import { albumIdOf, BANDCAMP, samePage } from './bandcampPreview';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { requestOpen } from './requestState';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';
  import { askedFor, wantOf, type Wantable } from './wantPath';

  let {
    page,
    artist = null,
    album = null,
    onclose,
  }: {
    page: string | null;
    /** Its names where its album's own can't be read (Bandcamp switched off or holding): a track's menu gives none. */
    artist?: string | null;
    album?: string | null;
    onclose: () => void;
  } = $props();
  let suggestion = $state<{ album: DiscoverAlbum; asked: boolean; may: boolean } | null>(null);
  /** Not a suggestion: what its Want sends, whether the account has asked for it, may ask. */
  let other = $state<{ want: Wantable; asked: boolean; may: boolean } | null>(null);
  // Asked again only when what it is about changes (the owner, 2026-10-30: Want and Not interested blinked in and out
  // of the player's menu). The props are read through the playing track, a new object at each report of its player,
  // so an effect on them alone ran, and cleared both, every few seconds. An answer for what it was about before is
  // dropped.
  const about = $derived(page ? JSON.stringify([page, artist, album]) : null);
  $effect(() => {
    const key = about;
    suggestion = null;
    other = null;
    if (!key) return;
    const [p, ar, al] = JSON.parse(key) as [string, string | null, string | null];
    let current = true;
    getDiscover().then(
      (v) => {
        if (!current) return;
        const found = v.found?.albums.find((a) => a.bandcamp && samePage(a.bandcamp, p));
        const r = found ? v.requests?.[found.key] : undefined;
        if (found) return (suggestion = { album: found, asked: requestOpen(r), may: !!v.may_request });
        const may = !!v.may_request;
        // Its album as Bandcamp's page and Cratarr name it: the record that holds the page, else the group it was found by,
        // and the album's own artist and title, with where the viewer's request of it stands (G3). Not read: its names as
        // given, by its page, whose request the Requests list says.
        albumView(BANDCAMP, albumIdOf(p))
          .then(
            (b): { want: Wantable; asked: boolean } | null =>
              b.artist ? { want: { record: b.record, release_group: b.release_group, page: p, artist: b.artist, album: b.title }, asked: requestOpen(b.request) } : null,
            () => null,
          )
          .then((viewed) => {
            if (!current) return;
            if (viewed) return (other = { want: viewed.want, asked: viewed.asked, may });
            const w: Wantable | null = ar && al ? { page: p, artist: ar, album: al } : null;
            if (!w) return;
            other = { want: w, asked: false, may };
            getRequests().then(
              (q) => current && (other = { want: w, may, asked: askedFor(q.mine, w) }),
              () => {},
            );
          });
      },
      () => current && (suggestion = null),
    );
    return () => (current = false);
  });
  async function wantOther() {
    const w = other?.want;
    if (!w) return;
    onclose();
    try {
      const { request } = await wantAlbum(wantOf(w));
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      sayWantFailed(e);
    }
  }
  async function want() {
    const a = suggestion?.album;
    if (!a) return;
    onclose();
    try {
      const { request } = await wantDiscover(a.key);
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      sayWantFailed(e);
    }
  }
  async function no() {
    const a = suggestion?.album;
    if (!a) return;
    onclose();
    try {
      const { undo } = await notDiscover(a.key);
      withUndo('Not interested.', [undo]);
    } catch (e) {
      toast.fail(e, 'Could not leave it out');
    }
  }
</script>

<MenuGroup label="Discover">
  {#if suggestion}
    {#if suggestion.asked}
      <MenuEntry
        icon="inbox"
        label="Requested"
        onclick={() => {
          onclose();
          location.hash = hrefOf('requests');
        }}
      />
    {:else}
      <MenuEntry icon="circlePlus" label="Want" disabled={!suggestion.may} onclick={want} />
    {/if}
    <MenuEntry icon="thumbsDown" label="Not interested" onclick={no} />
  {:else if other}
    {#if other.asked}
      <MenuEntry
        icon="inbox"
        label="Requested"
        onclick={() => {
          onclose();
          location.hash = hrefOf('requests');
        }}
      />
    {:else}
      <MenuEntry icon="circlePlus" label="Want" disabled={!other.may} onclick={wantOther} />
    {/if}
  {/if}
</MenuGroup>
