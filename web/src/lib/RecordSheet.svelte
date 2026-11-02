<script lang="ts">
  // A record of a list opened (the owner, 2026-11-02: in a crate's records it replaced the crate's list in place; "it
  // should open a sheet with all the tracks in that playlist of that record"; a flow's front cover too): the record on
  // its card (its cover, artist, how much of it the list holds), Play and Shuffle of those tracks, then each as the
  // crate search's row (lib/SnippetRows): a tap plays the record's tracks here from it, its round button its snippet on
  // its waveform, ⋮ its menu (`onmenu`).
  import { coverUrl, type PlaylistRow } from './api';
  import MenuSheet from './MenuSheet.svelte';
  import { playCollection } from './play.svelte';
  import SnippetRows from './SnippetRows.svelte';
  import { toast } from './toast.svelte';

  let {
    source,
    title,
    chips,
    items,
    onmenu = null,
    onclose,
  }: {
    source: string;
    title: string;
    chips: string[];
    items: { r: PlaylistRow; i: number }[];
    onmenu?: ((r: PlaylistRow) => void) | null;
    onclose: () => void;
  } = $props();

  const first = $derived(items.find((x) => x.r.id && !x.r.missing)?.r ?? null);
  const ids = $derived(items.flatMap((x) => (x.r.id && !x.r.missing ? [x.r.id] : [])));
  let working = $state(false);
  async function play(how: 'play' | 'shuffle', from?: string) {
    working = true;
    try {
      await playCollection({ kind: 'tracks', source, ids: [...ids] }, how, title, from);
      onclose();
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      working = false;
    }
  }
</script>

<MenuSheet
  label={title}
  {title}
  {chips}
  images={first?.id ? [coverUrl({ source, id: first.id })] : null}
  actions={[
    { icon: 'play', label: 'Play', disabled: working || !ids.length, onclick: () => play('play') },
    { icon: 'shuffle', label: 'Shuffle', disabled: working || !ids.length, onclick: () => play('shuffle') },
  ]}
  {onclose}
>
  <div class="tracks">
    <SnippetRows {source} {items} key={title} label={`Its tracks of ${title}`} onplay={(r) => r.id && play('play', r.id)} {onmenu} />
  </div>
</MenuSheet>

<style>
  .tracks {
    padding: 0 16px 16px;
  }
</style>
