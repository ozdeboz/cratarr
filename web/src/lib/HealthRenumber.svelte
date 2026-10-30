<script lang="ts">
  // An album whose track numbers are off, Health's third level (the owner, 2026-10-25; unnumbered tracks the same
  // way): the numbers a release has for it, looked up as a task (MusicBrainz, then Discogs at its pace), each
  // track now and after, then Fix writes exactly those (Undo puts the old ones back). Where they come from and whether
  // its discs stay (a CD's) or run on as one from 01 (vinyl, cassette) is said over the list.
  import { lookRenumber, missingCall, renumber, type Renumbering } from './api';
  import Icon from './Icon.svelte';
  import Meter from './Meter.svelte';
  import { loadHealth } from './healthReport.svelte';
  import { me } from './me.svelte';
  import PageLoading from './PageLoading.svelte';
  import { progressOf, tasks } from './tasks.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { album, title, ongone }: { album: string; title: string; ongone: () => void } = $props();

  let taskId = $state<number | null>(null);
  let error = $state<string | null>(null);
  let preview = $state<Renumbering | null>(null);
  let working = $state(false);
  const look = $derived(taskId !== null ? tasks.of(`renumber:${album}`) : null);
  const progress = $derived(look && look.finished === null ? progressOf(look) : null);
  $effect(() => tasks.follow());
  $effect(() => {
    const id = album;
    preview = null;
    error = null;
    lookRenumber(id).then(
      ({ task }) => {
        tasks.begun(task);
        taskId = task.id;
      },
      (e) => (error = missingCall(e) ? "This server can't renumber yet." : e instanceof Error ? e.message : 'The numbers could not be looked up.'),
    );
  });
  $effect(() => {
    const t = look;
    if (!t || t.id !== taskId || t.finished === null) return;
    taskId = null;
    if (t.error) error = t.error;
    else preview = t.result as Renumbering;
  });

  const FROM = { musicbrainz: 'MusicBrainz', discogs: 'Discogs', files: 'the files’ order' } as const;
  const at = (x: { disc: number | null; track: number | null }, discs: boolean) =>
    x.track === null ? '–' : `${discs && x.disc ? `${x.disc}-` : ''}${String(x.track).padStart(2, '0')}`;
  const changes = $derived(preview ? preview.tracks.filter((t) => t.now.disc !== t.to.disc || t.now.track !== t.to.track).length : 0);

  async function fix() {
    if (!preview || working) return;
    working = true;
    try {
      const { undo } = await renumber(album, preview.tracks.map((t) => ({ id: t.id, ...t.to })), preview.totals);
      withUndo(`Renumbered ${title}.`, undo, { after: loadHealth });
      loadHealth();
      ongone();
    } catch (e) {
      toast.fail(e, "The numbers weren't changed");
    } finally {
      working = false;
    }
  }
</script>

{#if error}
  <p class="muted">{error}</p>
{:else if !preview}
  <PageLoading>
    {#if progress}{#if progress.value !== null}<Meter label={progress.line} value={progress.value} />{:else}<p class="muted">{progress.line}</p>{/if}{/if}
  </PageLoading>
{:else}
  {@const p = preview}
  {@const many = p.discs === 'kept' && p.totals.discs > 1}
  <p class="from">
    As {FROM[p.from]}{#if p.release} has {p.release.title}{[p.release.format, p.release.country, p.release.year].filter(Boolean).length ? ` (${[p.release.format, p.release.country, p.release.year].filter(Boolean).join(', ')})` : ''}{/if}.
    {p.discs === 'kept' ? `Its discs stay (${p.why}).` : `One disc, from 01 (${p.why}).`}
  </p>
  <ol class="tracks">
    {#each p.tracks as t (t.id)}
      {@const moved = t.now.disc !== t.to.disc || t.now.track !== t.to.track}
      <li class:moved>
        <span class="nums"><span class="now">{at(t.now, many)}</span>{#if moved}<Icon name="arrowRight" size={14} /><b>{at(t.to, many)}</b>{/if}</span>
        <span class="name">{t.title ?? t.file}</span>
      </li>
    {/each}
  </ol>
  {#if me.admin}
    <button class="act" disabled={working || !changes} aria-busy={working} onclick={fix}>{changes ? `Fix ${changes} ${changes === 1 ? 'track' : 'tracks'}` : 'Nothing to change'}</button>
  {/if}
{/if}

<style>
  p {
    margin: 0;
  }

  .muted {
    color: var(--muted);
  }

  .from {
    color: var(--muted);
  }

  /* Each track: its number now, and after where it changes, then its title. */
  .tracks {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tracks li {
    display: flex;
    gap: 12px;
    align-items: baseline;
    min-width: 0;
  }

  .nums {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 7ch;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }

  .moved .nums b {
    color: var(--accent-ink);
  }

  .moved .now {
    text-decoration: line-through;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* As the panel's own way onward (lib/HealthPanel .act). */
  .act {
    justify-self: start;
    padding: 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .act:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
