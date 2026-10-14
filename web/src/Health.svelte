<script lang="ts">
  // Health (decision 0025): the library's files the catalogue could not read properly, by kind, worst first. What is
  // about playlists (missing and repeated tracks) is repaired in the panel's Repair tab.
  import { getFileProblems, type FileProblem } from './lib/api';
  import Notice from './lib/Notice.svelte';

  let problems = $state<FileProblem[] | null>(null);
  let error = $state<string | null>(null);
  $effect(() => {
    getFileProblems().then(
      (p) => (problems = p),
      (e) => (error = e instanceof Error ? e.message : 'Unexpected error'),
    );
  });

  const KINDS: { kind: FileProblem['kind']; title: string; hint: string }[] = [
    { kind: 'broken', title: "Files that can't be read", hint: 'Not readable as audio at all. Usually a damaged or incomplete file; replace it.' },
    {
      kind: 'damaged_tag',
      title: 'Files with a damaged tag',
      hint: 'One tag field is broken and was skipped; the music and the other tags are fine. Saving the tags again in a tag editor fixes it.',
    },
    { kind: 'no_tags', title: 'Files without tags', hint: 'No artist, title or other tags at all.' },
    { kind: 'no_length', title: 'Files without a length', hint: "The audio's length could not be read." },
  ];
  const ofKind = (k: FileProblem['kind']) => (problems ?? []).filter((p) => p.kind === k);
  const fileOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
  const albumOf = (path: string) => path.slice(0, path.lastIndexOf('/'));
</script>

<main>
  {#if error}
    <Notice text={error} />
  {:else if !problems}
    <p class="empty">Loading…</p>
  {:else if !problems.length}
    <p class="empty">Every file in the library reads properly.</p>
  {:else}
    {#each KINDS as k (k.kind)}
      {@const list = ofKind(k.kind)}
      {#if list.length > 0}
        <details class="files">
          <summary>{k.title} <span class="n">{list.length}</span></summary>
          <p class="hint">{k.hint}</p>
          <ul class="rows">
            {#each list as p (p.folder + p.rel)}
              <li title={p.folder}>
                <strong>{fileOf(p.rel)}</strong>
                <div class="pair">{albumOf(p.rel)}{#if p.detail}&nbsp;· {p.detail}{/if}</div>
              </li>
            {/each}
          </ul>
        </details>
      {/if}
    {/each}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .hint,
  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .empty {
    margin-top: 32px;
  }

  .n {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  details.files {
    margin: 10px 0;
  }

  details.files summary {
    cursor: pointer;
    font-weight: 600;
  }

  .rows {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
  }

  .rows > li {
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .pair {
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }
</style>
