<script lang="ts">
  // A record waiting in Incoming as its intake card (0.14; the owner, 2026-11-02: Health's "Records waiting for you",
  // checks on top, tracks under): where it came from and its format, each check with what it found (a slow one waiting
  // while its tracks are still heard: asked again), then its tracks, each its length beside the reference's, its peaks
  // and three parts of it to hear (start, middle, end). Keep, Replace and Delete stay in its album menu
  // (lib/HealthIncoming), Keep suggested here where every check agrees.
  import { getIntakeCard, type CardCheck, type IntakeCard, type IntakeTrack } from './api';
  import { plural } from './insights.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import { clockText } from './remote.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import SnippetMark from './SnippetMark.svelte';
  import { heardShare, snippetAtOn, toggleSnippetAt } from './snippetPlayer.svelte';

  let { source, album }: { source: string; album: string } = $props();

  let card = $state<IntakeCard | null>(null);
  let failed = $state<string | null>(null);
  let again = $state(0);
  $effect(() => {
    void again;
    let gone = false;
    let wait: ReturnType<typeof setTimeout> | undefined;
    getIntakeCard(source, album).then(
      (c) => {
        if (gone) return;
        card = c;
        failed = null;
        // Its tracks still being heard (one record at a time, in the background): asked again for the rest.
        if (c.checks.some((x) => x.state === 'waiting')) wait = setTimeout(() => again++, 3000);
      },
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => {
      gone = true;
      clearTimeout(wait);
    };
  });

  const NAME: Record<CardCheck['key'], string> = {
    reference: 'Reference',
    acoustid: 'AcoustID',
    sound: 'Sound',
    one_record: 'One record',
    folder_name: 'Folder name',
    one_user: 'One Soulseek user',
    in_library: 'In the library',
  };
  const LEAD: Record<CardCheck['state'], string> = { green: 'on', amber: 'warn', red: 'bad', waiting: 'off' };

  const time = (ms: number | null | undefined) => (ms ? clockText(ms / 1000) : '–');
  /** Its length against the reference's: said where they differ by more than two seconds. */
  const off = (t: IntakeTrack) => !!t.length_ms && !!t.reference_ms && Math.abs(t.length_ms - t.reference_ms) > 2000;
  const number = (t: IntakeTrack, discs: boolean) => (t.position ? `${discs && t.disc ? `${t.disc}-` : ''}${String(t.position).padStart(2, '0')}` : '');
  const PARTS = [
    ['start', 'Start'],
    ['middle', 'Middle'],
    ['end', 'End'],
  ] as const;
</script>

{#if failed && !card}
  <Notice text={failed} onretry={() => again++} />
{:else if !card}
  <PageLoading />
{:else}
  {@const c = card}
  {@const discs = new Set(c.tracks.map((t) => t.disc ?? 1)).size > 1}
  <p class="from">{[c.from.said, c.format].filter(Boolean).join(' · ')}</p>
  <Group>
    {#if c.suggested === 'keep'}<Item lead="on" name="Adding suggested" line="Every check agrees" />{/if}
    {#each c.checks as x (x.key)}
      <Item lead={LEAD[x.state]} name={NAME[x.key] ?? x.key} line={x.said} tone={x.state === 'red' ? 'bad' : x.state === 'amber' ? 'warn' : null} busy={x.state === 'waiting'} wrap />
    {/each}
  </Group>
  <Group title={plural(c.tracks.length, 'track')}>
    <ol class="tracks">
      {#each c.tracks as t (t.id)}
        <!-- The copy waiting is what the card is about, whichever plays. -->
        {@const heard = { source: c.source, id: t.id, from: c.library, name: t.title, duration_secs: t.length_ms ? t.length_ms / 1000 : null }}
        <li>
          <span class="num">{number(t, discs)}</span>
          <span class="name">{t.title}</span>
          <span class="len" class:off={off(t)}
            >{time(t.length_ms)}{#if t.reference_ms && off(t)}<small> · ref {time(t.reference_ms)}</small>{/if}</span
          >
          {#if t.peaks?.length}
            <svg class="peaks" viewBox="0 0 {t.peaks.length} 255" preserveAspectRatio="none" aria-hidden="true">
              {#each t.peaks as p, i (i)}<rect x={i + 0.15} y={(255 - p) / 2} width="0.7" height={Math.max(p, 6)} />{/each}
            </svg>
          {/if}
          <span class="parts">
            {#each PARTS as [part, label] (part)}
              {@const on = snippetAtOn(heard, t.snippets[part])}
              <button aria-label={`${label} of ${t.title}`} title={label} onclick={() => toggleSnippetAt(heard, t.snippets[part])}
                ><SnippetMark open={on} share={on ? heardShare() : 0} size={26} /></button
              >
            {/each}
          </span>
        </li>
      {/each}
    </ol>
  </Group>
{/if}

<style>
  .from {
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }

  .tracks {
    display: grid;
    margin: 0;
    padding: 4px 0;
    list-style: none;
  }

  /* Number, title, length; its peaks and parts under them. */
  li {
    display: grid;
    grid-template-columns: 2.4em minmax(0, 1fr) auto;
    grid-template-areas:
      'num name len'
      '. peaks parts';
    align-items: center;
    gap: 2px 10px;
    padding: 8px 14px;
  }

  .num {
    grid-area: num;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .name {
    grid-area: name;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .len {
    grid-area: len;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .len.off {
    color: var(--warn-ink);
  }

  .peaks {
    grid-area: peaks;
    width: 100%;
    height: 22px;
    fill: var(--muted);
    opacity: 0.7;
  }

  .parts {
    grid-area: parts;
    display: flex;
    gap: 2px;
  }

  .parts button {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: none;
    color: inherit;
    cursor: pointer;
  }
</style>
