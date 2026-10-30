<script lang="ts">
  // A crate's conditions being made or changed (decision 0024), in the crate's panel in its edit mode, where its tracks
  // were (lib/PlaylistView.svelte): the same editor for a new crate and one's own. Its conditions all hold (AND), each
  // can be negated or muted; OR within an axis by choosing several values or a range. They are the Filter sheet's cards
  // (lib/ConditionList, decision 0061), on the whole library's tracks (no Any or All). The controls come from the
  // server's axis list (kind and domain), so a new axis needs nothing here. Below them, the unsaved crate's first tracks
  // (after 300 ms); its size goes to the panel (`total`), which shows it under the name.
  import { coverUrl, getCrateAxes, previewCrate, type Axis, type PlaylistRow } from './api';
  import ConditionList from './ConditionList.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { prefs, type FilterCondition } from './prefs.svelte';
  import { toast } from './toast.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';

  let { conditions = $bindable(), total = $bindable(null) }: { conditions: FilterCondition[]; total?: number | null } = $props();

  // The axes: their kinds and domains in this library.
  let axes = $state<Axis[] | null>(null);
  $effect(() => {
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      (e) => {
        axes ??= [];
        toast.fail(e, "The crate's axes couldn't be loaded");
      },
    );
  });
  // The live preview.
  let preview = $state<{ total: number; rows: PlaylistRow[]; source: string } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    const asked = JSON.parse(k) as FilterCondition[];
    const t = setTimeout(() => {
      previewCrate(asked, 50, prefs.source || undefined, { without: hiddenLibraries() }).then(
        (p) => {
          if (k !== key) return;
          preview = p;
          previewError = null;
          total = p.total;
        },
        (e) => {
          if (k !== key) return;
          previewError = e instanceof Error ? e.message : 'Unexpected error';
          total = null;
        },
      );
    }, 300);
    return () => clearTimeout(t);
  });
</script>

<div class="editor">
  <ConditionList bind:conditions {axes} crate />

  {#if previewError}
    <p class="error" role="alert">{previewError}</p>
  {:else if preview}
    <ol class="preview" aria-label="The crate's first tracks">
      {#each preview.rows as r, i (i)}
        {@const src = preview.source}
        <li>
          <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || !!r.missing}>
            {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} />{/snippet}
          </Row>
        </li>
      {/each}
    </ol>
    {#if preview.total > preview.rows.length}<p class="note">and {(preview.total - preview.rows.length).toLocaleString()} more</p>{/if}
  {/if}
</div>

<style>
  .editor {
    padding: 0 12px;
  }

  .preview {
    margin: 18px -12px 0;
    padding: 0;
    list-style: none;
  }

  .note {
    margin: 4px 0;
    color: var(--muted);
    font-size: 13px;
  }

  .error {
    color: var(--danger-ink);
  }
</style>
