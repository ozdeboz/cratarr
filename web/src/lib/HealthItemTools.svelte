<script lang="ts">
  // What can be done for one Health item (F28.2): Health's row opened in place and a record's health sheet draw it alike, by one
  // when-table (lib/healthTools), in today's order. The sheets its tools open are its host's, which it is told of.
  import type { HealthItem } from './api';
  import type { ReleaseTrack } from './replaceTrack';
  import { offers } from './capabilities.svelte';
  import { me } from './me.svelte';
  import { openFixes } from './fileRuns.svelte';
  import { EVENTS, fullOf, itemTools, type ToolKind } from './healthTools';
  import HealthEvents from './HealthEvents.svelte';
  import HealthIncoming from './HealthIncoming.svelte';
  import HealthMissing from './HealthMissing.svelte';
  import HealthReconnect from './HealthReconnect.svelte';
  import HealthRenumber from './HealthRenumber.svelte';
  import RecordReferences from './RecordReferences.svelte';
  import Item from './settings/Item.svelte';

  let {
    problem,
    item: x,
    here = null,
    finds,
    fixable,
    findingKey,
    tracks,
    asking,
    askedFull,
    go = null,
    flush = false,
    onaskfull,
    onsearch,
    onbetter,
    oncover,
    ongone,
  }: {
    /** The finding's key (a files kind's `files-<kind>`). */
    problem: string;
    item: HealthItem;
    /** The record a sheet is over: no Go to it (F28.3). */
    here?: string | null;
    finds: boolean;
    fixable: boolean;
    findingKey: string;
    /** Its missing tracks as their release has them, once read; until then the report's names for them. */
    tracks: ReleaseTrack[] | null;
    asking: boolean;
    askedFull: string | null;
    go?: { label: string; go: () => void } | null;
    /** In a sheet: no row's indent. */
    flush?: boolean;
    onaskfull: (x: HealthItem) => void;
    onsearch: (s: { album: string; title: string; recording: string }) => void;
    onbetter: (b: { source: string; album: string; track: string }) => void;
    oncover: (c: { id: string; title: string }) => void;
    ongone: () => void;
  } = $props();

  const tools = $derived(itemTools(problem, x, { admin: me.admin, finds, renumber: offers('renumber'), fixable, here }));
  const has = (k: ToolKind) => tools.includes(k);
  const clock = (ms: number | null) => (ms ? `${Math.floor(ms / 60000)}:${String(Math.round((ms / 1000) % 60)).padStart(2, '0')}` : '');
</script>

<div class="opened" class:flush>
  {#if has('playlistMissing')}<div class="tool"><HealthMissing playlist={x.playlist!} /></div>{/if}
  <!-- A track of Missing music: what it is now, by a person's word (lib/HealthReconnect). -->
  {#if has('reconnect')}<div class="tool"><HealthReconnect track={x.track!} title={x.title} ongone={ongone} /></div>{/if}
  {#if has('incoming')}<div class="tool"><HealthIncoming source={x.album![0]} album={x.album![1]} ongone={ongone} /></div>{/if}
  <!-- Track numbers off (unnumbered ones too), disc numbers: renumbered as a release has them (lib/HealthRenumber). -->
  {#if has('renumber')}<div class="tool"><HealthRenumber album={x.album![1]} title={x.title} ongone={ongone} /></div>{/if}
  <!-- Records none speaks for: a row per source, Use this, Paste a link, Checked by hand (lib/RecordReferences). -->
  {#if has('references')}<div class="tool"><RecordReferences album={x.album![1]} /></div>{/if}
  {#if has('events')}<div class="tool"><HealthEvents {...EVENTS[problem]} value={x.title} /></div>{/if}
  {#if has('missingTracks')}
    <!-- An album missing tracks: each one, looked for on Soulseek one at a time (lib/ReplaceTrackSheet); the whole album asked for. -->
    {#if tracks}
      {#each tracks as t (t.recording)}
        <Item name={`${t.disc > 1 ? `${t.disc}-` : ''}${t.position}. ${t.title}`} line={clock(t.lengthMs) || null}>
          {#if finds && x.album}<button onclick={() => onsearch({ album: x.album![1], title: x.title, recording: t.recording })}>Search</button>{/if}
        </Item>
      {/each}
    {:else}
      {#each x.missing ?? [] as m (m)}
        <Item name={m}>{#if finds && x.album?.[0] === 'cratarr'}<button disabled aria-busy="true">Search</button>{/if}</Item>
      {/each}
    {/if}
  {/if}
  {#if has('askFull')}
    {#if x.request || askedFull === fullOf(x)}
      <Item name="Asked for the full album" />
    {:else}
      <Item name="Ask for the full album" busy={asking} disabled={asking} onclick={() => onaskfull(x)} />
    {/if}
  {/if}
  <!-- A folder's files not heard yet, by name. -->
  {#if has('files')}{#each x.files ?? [] as file (file)}<Item name={file} wrap />{/each}{/if}
  <!-- A file's own row (Files' kinds): where it is and what is wrong, whole, and its fix where the finding has one. -->
  {#if has('fileLine')}<Item name={x.line!} wrap />{/if}
  {#if has('fixes')}<Item name="Review the fixes" line="Certain ones ticked, each written into its files" onclick={() => openFixes(findingKey)} />{/if}
  <!-- A track's file cut off or breaking off: a better copy of it, as the track's menu finds one. -->
  {#if has('betterCopy')}<Item name="Find a better copy" onclick={() => onbetter({ source: x.album![0], album: x.album![1], track: x.track! })} />{/if}
  <!-- No cover: Change cover, straight on its choices. -->
  {#if has('changeCover')}<Item name="Change cover" onclick={() => oncover({ id: x.album![1], title: x.title })} />{/if}
  {#if has('goTo') && go}<Item name={go.label} onclick={go.go} />{/if}
</div>

<style>
  .opened {
    padding-left: 58px;
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }

  .opened.flush {
    padding-left: 0;
    background: none;
  }

  .opened > :global(* + *) {
    border-top: 1px solid var(--border);
  }

  .tool {
    display: grid;
    gap: 12px;
    padding: 12px 16px 12px 0;
  }
</style>
