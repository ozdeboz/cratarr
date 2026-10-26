<script lang="ts">
  // Download once, in a menu (the owner, 2026-10-24): a one-time copy on this device, as against Sync to this device
  // (decision 0038), which follows the item. The menu stays open and the row itself fills as the tracks come in, its
  // label counting them ("Downloading 3 of 12"); opened again later, it shows the same. Some tracks failed: Retry; some
  // not there yet: the rest; all there: no row (Remove from this device stays last in the menu).
  import { askDownloads, download, downloads, inApp, type Downloadable } from './app.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { plural } from './insights.svelte';

  let { tracks }: { tracks: Downloadable[] } = $props();

  // What the app tells of each of them.
  const on = $derived(new Map(downloads.tracks.map((t) => [`${t.source}/${t.id}`, t.download])));
  const of = $derived(tracks.map((t) => ({ t, d: on.get(`${t.source}/${t.id}`) })));
  const done = $derived(of.filter((x) => x.d?.state === 'done').length);
  const going = $derived(of.filter((x) => x.d?.state === 'downloading' || x.d?.state === 'waiting'));
  const failed = $derived(of.filter((x) => x.d?.state === 'failed').map((x) => x.t));
  const missing = $derived(of.filter((x) => !x.d).map((x) => x.t));
  // Asked for here: counted from the tap, before the app has told anything back.
  let asked = $state(false);
  const coming = $derived(going.length > 0 || (asked && missing.length > 0));
  const progress = $derived(
    tracks.length ? (done + going.reduce((n, x) => n + (x.d?.state === 'downloading' ? (x.d.percent ?? 0) / 100 : 0), 0)) / tracks.length : 0,
  );

  if (inApp) askDownloads();
  // The app tells each track's change; its bytes in between only when asked.
  $effect(() => {
    if (!coming) return;
    const tick = setInterval(askDownloads, 1500);
    return () => clearInterval(tick);
  });

  function get(list: Downloadable[]) {
    asked = true;
    download(list);
  }
</script>

{#if inApp && tracks.length}
  {#if coming}
    <MenuEntry icon="arrowDownToLine" label={`Downloading ${done} of ${plural(tracks.length, 'track')}`} progress={progress} onclick={() => {}} />
  {:else if failed.length}
    <MenuEntry icon="arrowDownToLine" label={`Retry ${plural(failed.length, 'failed track')}`} onclick={() => get([...failed, ...missing])} />
  {:else if missing.length === tracks.length}
    <MenuEntry icon="arrowDownToLine" label="Download once" onclick={() => get(tracks)} />
  {:else if missing.length}
    <MenuEntry icon="arrowDownToLine" label={`Download the other ${plural(missing.length, 'track')} once`} onclick={() => get(missing)} />
  {/if}
{/if}
