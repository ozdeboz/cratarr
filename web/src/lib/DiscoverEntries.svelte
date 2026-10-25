<script lang="ts">
  // A menu's Discover group for something on Bandcamp (decision 0071, the owner 2026-10-25: "the Discover buttons need
  // their own section or group"): the Discover suggestion its album page is, Want (or Requested) and Not interested,
  // as on Discover's own menu. Not a suggestion (the owner, 2026-10-25: "When playing any Bandcamp release, its context
  // menu should offer the Want option"): Want for it by its artist and title (or Requested), as for any missing album.
  // The player's track menu and a Bandcamp album's.
  import { getDiscover, getRequests, notDiscover, wantAlbum, wantDiscover, type DiscoverAlbum } from './api';
  import { samePage } from './bandcampPreview';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let {
    page,
    artist = null,
    album = null,
    onclose,
  }: { page: string | null; artist?: string | null; album?: string | null; onclose: () => void } = $props();
  let suggestion = $state<{ album: DiscoverAlbum; asked: boolean; may: boolean } | null>(null);
  /** Not a suggestion: Want by its names, whether the account has asked for it, may ask. */
  let other = $state<{ asked: boolean; may: boolean } | null>(null);
  const open = (status: string) => status !== 'cancelled' && status !== 'declined';
  const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
  $effect(() => {
    const [p, ar, al] = [page, artist, album];
    suggestion = null;
    other = null;
    if (!p) return;
    getDiscover().then(
      (v) => {
        const found = v.found?.albums.find((a) => a.bandcamp && samePage(a.bandcamp, p));
        const r = found ? v.requests?.[found.key] : undefined;
        if (found) return (suggestion = { album: found, asked: !!r && open(r.status), may: !!v.may_request });
        if (!ar || !al) return;
        const may = !!v.may_request;
        other = { asked: false, may };
        getRequests().then(
          (q) => (other = { may, asked: q.mine.some((m) => open(m.status) && same(m.artist, ar) && same(m.album, al)) }),
          () => {},
        );
      },
      () => (suggestion = null),
    );
  });
  async function wantOther() {
    if (!artist || !album) return;
    onclose();
    try {
      const { request } = await wantAlbum({ artist, album });
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
    } catch (e) {
      toast.fail(e, 'Could not ask for it');
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
      toast.fail(e, 'Could not ask for it');
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
          location.hash = '#/requests';
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
          location.hash = '#/requests';
        }}
      />
    {:else}
      <MenuEntry icon="circlePlus" label="Want" disabled={!other.may} onclick={wantOther} />
    {/if}
  {/if}
</MenuGroup>
