<script lang="ts">
  // Download… to this device (the owner, 2026-10-24; in Download…'s step since 0.10, lib/DownloadStep): a one-time
  // copy on this device, as against Sync to this device (decision 0038), which follows the item. The step stays open
  // and the row itself fills as the tracks come in, its
  // label counting them ("Downloading 3 of 12"); opened again later, it shows the same. Some tracks failed: Retry; some
  // not there yet: the rest; all there: no row (Remove from this device stays last in the menu). In the installed page
  // (0.9.8, lib/offline) the same, kept in this browser, and all there: Remove from this browser.
  import { askDownloads, download, downloads, inApp, type Downloadable } from './app.svelte';
  import { forgetOffline, installed, keepOffline, keyOf, offline } from './offline.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { plural } from './insights.svelte';

  let { tracks }: { tracks: Downloadable[] } = $props();

  // What the app tells of each of them; in the installed page, what this browser holds and fetches.
  const pwa = !inApp && installed;
  type State = { state: 'done' | 'downloading' | 'waiting' | 'failed'; percent?: number | null };
  const on = $derived(
    pwa
      ? new Map<string, State>(
          tracks.flatMap((t): [string, State][] => {
            const k = keyOf(t);
            if (offline.kept[k]) return [[k, { state: 'done' }]];
            const f = offline.fetching[k];
            return f ? [[k, { state: f.state, percent: f.percent ?? null }]] : [];
          }),
        )
      : new Map<string, State>(downloads.tracks.map((t) => [`${t.source}/${t.id}`, t.download as State])),
  );
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
    if (!coming || pwa) return;
    const tick = setInterval(askDownloads, 1500);
    return () => clearInterval(tick);
  });

  function get(list: Downloadable[]) {
    asked = true;
    if (pwa) void keepOffline(list);
    else download(list);
  }
</script>

{#if (inApp || pwa) && tracks.length}
  {#if coming}
    <MenuEntry icon="arrowDownToLine" label={`Downloading ${done} of ${plural(tracks.length, 'track')}`} progress={progress} onclick={() => {}} />
  {:else if failed.length}
    <MenuEntry icon="arrowDownToLine" label={`Retry ${plural(failed.length, 'failed track')}`} onclick={() => get([...failed, ...missing])} />
  {:else if missing.length === tracks.length}
    <MenuEntry icon="arrowDownToLine" label={`Download ${plural(tracks.length, 'track')}`} onclick={() => get(tracks)} />
  {:else if missing.length}
    <MenuEntry icon="arrowDownToLine" label={`Download the other ${plural(missing.length, 'track')}`} onclick={() => get(missing)} />
  {:else if pwa}
    <MenuEntry icon="trash" label="Remove from this browser" onclick={() => void forgetOffline(tracks)} />
  {:else}
    <!-- All there already: said, so the step is never empty (Remove from this device is the menu's). -->
    <MenuEntry icon="check" label="All on this device" disabled onclick={() => {}} />
  {/if}
{/if}
