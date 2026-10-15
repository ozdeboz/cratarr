<script lang="ts">
  // Health (decision 0025): the library's files the catalogue could not read properly, by kind, worst first. What is
  // about playlists (missing tracks) is repaired in the panel's Repair tab. Each kind is a folded section of
  // file rows (lib/Section.svelte, lib/FileItem.svelte), as the Repair tab's.
  import { getFileProblems, type FileProblem } from './lib/api';
  import FileItem from './lib/FileItem.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Section from './lib/Section.svelte';

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
</script>

<main>
  {#if error}
    <Notice text={error} />
  {:else if !problems}
    <PageLoading />
  {:else if !problems.length}
    <p class="empty">Every file in the library reads properly.</p>
  {:else}
    {#each KINDS as k (k.kind)}
      {@const list = ofKind(k.kind)}
      {#if list.length > 0}
        <Section title={k.title} count={`${list.length} ${list.length === 1 ? 'file' : 'files'}`} hint={k.hint} open={false}>
          <ul class="rows">
            {#each list as p (p.folder + p.rel)}
              <li><FileItem path={p.rel} detail={p.detail ?? undefined} /></li>
            {/each}
          </ul>
        </Section>
      {/if}
    {/each}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .empty {
    margin-top: 32px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
