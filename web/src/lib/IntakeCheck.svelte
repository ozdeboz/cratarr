<script lang="ts">
  // An album's intake check, before Keep (0.9.7, the administrators' metadata workflow; the owner, 2026-10-26: new
  // music enters the library clean): what Cratarr would set (year, genre, album artist, MusicBrainz release, a cover
  // where it has none, a rule's names), each with where it comes from; the certain ones ticked, the likely ones to tick
  // (as Select's ticks, lib/RequestsPanel). Accept writes the ticked ones into the files (Undo puts them back); Keep then
  // moves the album into Music as ever. A genre's genres are picked (lib/GenrePick, 0.9.8).
  import { getIntake, missingCall, type Fix, type Fixes } from './api';
  import { apply, FIELD_LABEL } from './fixes.svelte';
  import GenrePick from './GenrePick.svelte';
  import Icon from './Icon.svelte';
  import { me } from './me.svelte';
  import PageLoading from './PageLoading.svelte';

  let { source, album, title }: { source: string; album: string; title: string } = $props();

  let found = $state<Fixes | null>(null);
  let gone = $state(false);
  let ticked = $state<Set<string>>(new Set());
  let wait: ReturnType<typeof setTimeout> | undefined;
  function read() {
    clearTimeout(wait);
    getIntake(source, album).then(
      (f) => {
        found = f;
        ticked = new Set(f.fixes.filter((x) => x.certainty === 'certain').map((x) => x.id));
        if (f.matching) wait = setTimeout(read, 2000);
      },
      (e) => (gone = missingCall(e)),
    );
  }
  $effect(() => {
    void album;
    found = null;
    read();
    return () => clearTimeout(wait);
  });
  function tick(id: string) {
    const s = new Set(ticked);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    ticked = s;
  }
  const shown = (v: string | null) => (v === null || v === '' ? '–' : v);
  // The genres picked, per fix (none touched: its strongest).
  let picked = $state<Record<string, string[]>>({});
  const pickedOf = (f: Fix) => picked[f.id] ?? [f.to];
  const nonePicked = $derived(!!found?.fixes.some((f) => ticked.has(f.id) && f.candidates?.length && !pickedOf(f).length));
  let working = $state(false);
  async function accept() {
    if (working || !ticked.size) return;
    working = true;
    const chosen = Object.fromEntries((found?.fixes ?? []).filter((f) => ticked.has(f.id) && f.candidates?.length).map((f) => [f.id, pickedOf(f)]));
    if (await apply({ ids: [...ticked], chosen }, '', `Set ${ticked.size} ${ticked.size === 1 ? 'value' : 'values'} of ${title}`)) read();
    working = false;
  }
</script>

{#if me.admin && !gone}
  <section class="intake" aria-label="Before it comes in">
    <h3>Before it comes in</h3>
    {#if !found || (found.matching && !found.fixes.length)}
      <PageLoading size={40} />
    {:else if !found.fixes.length}
      <p class="muted">Nothing to set: its tags are complete.</p>
    {:else}
      <ul class="fixes">
        {#each found.fixes as f (f.id)}
          {@const on = ticked.has(f.id)}
          <li>
            <span class="text">
              <span class="field">{FIELD_LABEL[f.field]}{#if f.certainty === 'likely'}<span class="likely"> · likely</span>{/if}</span>
              {#if f.field === 'cover'}
                <img class="pic" src={f.to} alt="" />
              {:else if f.candidates?.length}
                <GenrePick fix={f} bind:picked={() => pickedOf(f), (v) => (picked[f.id] = v)} />
              {:else}
                <span class="change"><span class="now">{shown(f.from)}</span><Icon name="arrowRight" size={14} /><b>{shown(f.to)}</b></span>
              {/if}
              <small class="why">{f.source} · {f.why}</small>
            </span>
            <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Set ${FIELD_LABEL[f.field]}`} onclick={() => tick(f.id)}
              ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
            >
          </li>
        {/each}
      </ul>
      <button class="act" disabled={working || !ticked.size || nonePicked} aria-busy={working} onclick={accept}>{ticked.size ? `Accept ${ticked.size}` : 'Nothing ticked'}</button>
    {/if}
  </section>
{/if}

<style>
  .intake {
    display: grid;
    gap: 12px;
  }

  h3 {
    margin: 0;
    font-size: 15px;
  }

  p {
    margin: 0;
  }

  .muted {
    color: var(--muted);
  }

  .fixes {
    display: grid;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .fixes li {
    display: flex;
    gap: 12px;
    align-items: center;
    min-width: 0;
  }

  .text {
    display: grid;
    flex: 1;
    gap: 4px;
    min-width: 0;
  }

  .field {
    color: var(--muted);
    font-size: 13px;
  }

  .likely {
    color: var(--warn-ink);
  }

  .change {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    overflow-wrap: anywhere;
  }

  .now,
  .why {
    color: var(--muted);
  }

  .pic {
    width: 96px;
    height: 96px;
    border-radius: 6px;
    object-fit: cover;
  }

  /* A row's tick (as Requests' lists). */
  .tick {
    display: grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .tick.on {
    color: var(--accent);
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
