<script lang="ts">
  // A menu's Discover group for something on Bandcamp (decision 0071, the owner 2026-10-25: "the Discover buttons need
  // their own section or group"): the Discover suggestion its album page is, Want (or Requested) and Not interested,
  // as on Discover's own menu. Nothing where it is no suggestion. The player's track menu and a Bandcamp album's.
  import { getDiscover, notDiscover, wantDiscover, type DiscoverAlbum } from './api';
  import { samePage } from './bandcampPreview';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { page, onclose }: { page: string | null; onclose: () => void } = $props();

  let suggestion = $state<{ album: DiscoverAlbum; asked: boolean; may: boolean } | null>(null);
  $effect(() => {
    const p = page;
    suggestion = null;
    if (!p) return;
    getDiscover().then(
      (v) => {
        const album = v.found?.albums.find((a) => a.bandcamp && samePage(a.bandcamp, p));
        const r = album ? v.requests?.[album.key] : undefined;
        suggestion = album ? { album, asked: !!r && r.status !== 'cancelled' && r.status !== 'declined', may: !!v.may_request } : null;
      },
      () => (suggestion = null),
    );
  });
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
  {/if}
</MenuGroup>
