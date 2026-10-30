<script lang="ts">
  // A work opened (0.12, decision 0086; the owner, 2026-10-30: "its recordings, each playable"): its recordings in the
  // library, one row each (its performers, album, year, length and cover); a tap plays that recording's movements in
  // their work's order, ⋮ opens its album's menu. The recording chosen (played, or the first) lists its movements under
  // it, a tap on one playing the recording from there; a long-press, right-click or ⋮ on a movement is its track's menu
  // (its snippet, Play next, Add to queue: the owner, 2026-10-30, works reachable). Cratarr's own library only.
  import { fromAction } from 'svelte/attachments';
  import AlbumMenu from './AlbumMenu.svelte';
  import { albumCoverUrl, getWork, type WorkRecording, type WorkView } from './api';
  import { longpress } from './longpress';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { playCollection } from './play.svelte';
  import { clockText } from './remote.svelte';
  import Row from './Row.svelte';
  import { toast } from './toast.svelte';
  import { inPartOrder, movementLabel } from './works';

  let { mbid, title, onclose }: { mbid: string; title: string; onclose: () => void } = $props();

  const SOURCE = 'cratarr';
  let view = $state<WorkView | null>(null);
  let failed = $state<string | null>(null);
  /** The recording whose movements show (its album). */
  let chosen = $state<string | null>(null);
  /** A movement's menu, or a recording's album's. */
  let trackMenu = $state<MenuTrack | null>(null);
  let albumMenu = $state<WorkRecording | null>(null);
  $effect(() => {
    const id = mbid;
    let gone = false;
    view = null;
    failed = null;
    getWork(id).then(
      (v) => {
        if (gone) return;
        view = v;
        chosen = v.recordings[0]?.album_id ?? null;
      },
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => (gone = true);
  });

  const name = $derived(view?.title ?? title);
  const tracksOf = (r: WorkRecording) => inPartOrder(r.tracks, view?.parts ?? []);
  const who = (r: WorkRecording) => (r.performers.length ? r.performers.join(' · ') : r.album);
  const length = (r: WorkRecording) => {
    const s = r.tracks.reduce((n, t) => n + (t.duration_secs ?? 0), 0);
    return s && r.tracks.every((t) => t.duration_secs) ? clockText(s) : undefined;
  };
  const partTitle = (id: string | null | undefined) => view?.parts.find((p) => p.mbid === id)?.title ?? null;

  /** Plays a recording's movements in order, from the first or the one given. */
  async function play(r: WorkRecording, from?: string) {
    chosen = r.album_id;
    const ids = tracksOf(r).map((t) => t.id);
    if (!ids.length) return;
    navigator.vibrate?.(8);
    try {
      await playCollection({ kind: 'tracks', source: SOURCE, ids }, 'play', `${name} · ${who(r)}`, from ?? ids[0]);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  type WorkTrack = WorkRecording['tracks'][number];
  /** A movement as a track's menu has it: named by its movement, the recording's performers as its artist. */
  const asTrack = (r: WorkRecording, t: WorkTrack, label: string): MenuTrack => ({
    source: SOURCE,
    id: t.id,
    name: label,
    artist: r.performers.length ? r.performers.join(', ') : null,
    album: r.album,
    album_id: r.album_id,
    duration_secs: t.duration_secs ?? null,
  });
  const press = (r: WorkRecording, t: WorkTrack, label: string) => fromAction(longpress, () => () => (trackMenu = asTrack(r, t, label)));
</script>

<MenuSheet
  label="Work"
  title={name}
  images={view ? view.recordings.slice(0, 1).map((r) => albumCoverUrl(SOURCE, r.album_id)) : []}
  chips={view?.composers.map((c) => c.name) ?? []}
  {onclose}
>
  {#if failed}
    <p class="note" role="alert">{failed}</p>
  {:else if !view}
    <div class="loading"><MarkLoader size={40} /></div>
  {:else if !view.recordings.length}
    <p class="note">No recordings of it in the library.</p>
  {:else}
    <ul class="rows" aria-label="Recordings of {name}">
      {#each view.recordings as r (r.album_id)}
        <li class:chosen={chosen === r.album_id}>
          <Row size="large" title={who(r)} sub={[r.album, r.year ? String(r.year) : null].filter(Boolean).join(' · ')} count={length(r)} label="Play {name} · {who(r)}" onclick={() => play(r)}>
            {#snippet lead()}<span class="art"><Cover src={albumCoverUrl(SOURCE, r.album_id)} /></span>{/snippet}
            {#snippet action()}<button class="tool" aria-label="{r.album}: its menu" onclick={() => (albumMenu = r)}><Icon name="moreVert" size={22} /></button>{/snippet}
          </Row>
          {#if chosen === r.album_id}
            <ol class="movements" aria-label="Movements">
              {#each tracksOf(r) as t, k (t.id)}
                {@const part = partTitle(t.part)}
                {@const label = part ? movementLabel(part, name) : `Part ${k + 1}`}
                <li>
                  <Row
                    size="medium"
                    title={label}
                    count={t.duration_secs ? clockText(t.duration_secs) : undefined}
                    onclick={() => play(r, t.id)}
                    attach={press(r, t, label)}
                  >
                    {#snippet action()}<button class="tool" aria-label="{label}: its menu" onclick={() => (trackMenu = asTrack(r, t, label))}><Icon name="moreVert" size={22} /></button>{/snippet}
                  </Row>
                </li>
              {/each}
            </ol>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</MenuSheet>
{#if trackMenu}<TrackMenu track={trackMenu} onclose={() => (trackMenu = null)} />{/if}
{#if albumMenu}{@const r = albumMenu}<AlbumMenu source={SOURCE} album={{ id: r.album_id, title: r.album, artist: r.performers.length ? r.performers.join(', ') : null }} onclose={() => (albumMenu = null)} />{/if}

<style>
  .rows,
  .movements {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* The chosen recording's movements, under it and set in, as an album's movements under their work. */
  .movements {
    margin: 2px 0 8px;
    padding-left: 24px;
  }

  .art {
    position: relative;
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 4px;
  }

  .tool {
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .note {
    margin: 16px;
    color: var(--muted);
    text-align: center;
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 24px;
  }
</style>
