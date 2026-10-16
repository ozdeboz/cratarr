<script lang="ts">
  // A crate's conditions being made or changed (decision 0024), in the crate's panel in its edit mode, where its tracks
  // were (lib/PlaylistView.svelte): the same editor for a new crate and one's own. Its conditions all hold (AND), each
  // can be negated; OR within an axis by choosing several values or a range. The controls come from the server's axis
  // list (kind and domain), so a new axis needs nothing here. Below them, the unsaved crate's first tracks (after
  // 300 ms); its size goes to the panel (`total`), which shows it under the name.
  import { coverUrl, getCrateAxes, previewCrate, type Axis, type Condition, type PlaylistRow } from './api';
  import Icon from './Icon.svelte';
  import { prefs } from './prefs.svelte';
  import { toast } from './toast.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';

  let { conditions = $bindable(), total = $bindable(null) }: { conditions: Condition[]; total?: number | null } = $props();

  // The axes: their kinds and domains in this library.
  let axes = $state<Axis[]>([]);
  $effect(() => {
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      (e) => toast.fail(e, "The crate's axes couldn't be loaded"),
    );
  });
  const axis = (key: string) => axes.find((a) => a.key === key);

  let adding = $state(false);
  function add(a: Axis) {
    const value: Condition['value'] = a.kind === 'yes_no' ? { kind: 'yes' } : a.kind === 'range' ? { kind: 'range', min: null, max: null } : { kind: 'any_of', values: [] };
    conditions = [...conditions, { axis: a.key, not: false, value }];
    adding = false;
  }
  const remove = (i: number) => (conditions = conditions.filter((_, j) => j !== i));
  function toggleValue(c: Condition, v: string) {
    if (c.value.kind !== 'any_of') return;
    c.value.values = c.value.values.includes(v) ? c.value.values.filter((x) => x !== v) : [...c.value.values, v];
  }
  // A values axis can have many (genres): a filter, and the chosen ones first.
  let filters = $state<Record<number, string>>({});
  function shown(c: Condition, i: number, a: Axis) {
    const q = (filters[i] ?? '').trim().toLowerCase();
    const chosen = c.value.kind === 'any_of' ? c.value.values : [];
    const rest = a.values.filter((v) => !chosen.includes(v) && (!q || v.toLowerCase().includes(q)));
    return [...chosen, ...rest.slice(0, q ? 60 : 24)];
  }
  const num = (s: string) => (s.trim() === '' || Number.isNaN(Number(s)) ? null : Number(s));

  // The live preview.
  let preview = $state<{ total: number; rows: PlaylistRow[]; source: string } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    const asked = JSON.parse(k) as Condition[];
    const t = setTimeout(() => {
      previewCrate(asked, 50, prefs.source || undefined).then(
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
  <ol class="conditions">
    {#each conditions as c, i (i)}
      {@const a = axis(c.axis)}
      <li class="cond" class:negated={c.not}>
        <div class="head">
          <button class="neg" aria-pressed={c.not} title="Negate this condition" onclick={() => (c.not = !c.not)}>{c.not ? 'NOT' : 'is'}</button>
          <strong>{a?.label ?? c.axis}</strong>
          {#if a?.kind === 'yes_no'}<span class="muted">yes</span>{/if}
          {#if a?.kind === 'membership' && c.value.kind === 'any_of' && !c.value.values.length}<span class="muted">any playlist</span>{/if}
          <button class="x" aria-label="Remove this condition" onclick={() => remove(i)}><Icon name="close" size={18} /></button>
        </div>
        {#if a && (a.kind === 'values' || a.kind === 'membership') && c.value.kind === 'any_of'}
          {#if a.values.length > 24}
            <input class="filter" type="search" placeholder="Find {a.label.toLowerCase()}" bind:value={filters[i]} />
          {/if}
          <div class="values">
            {#each shown(c, i, a) as v (v)}
              <button class="value" aria-pressed={c.value.values.includes(v)} onclick={() => toggleValue(c, v)}>{v}</button>
            {/each}
          </div>
        {:else if a && a.kind === 'range' && c.value.kind === 'range'}
          {@const r = c.value}
          <div class="range">
            <input type="number" inputmode="decimal" aria-label="From" placeholder={a.min === null ? 'from' : String(Math.floor(a.min))} value={r.min ?? ''} oninput={(e) => (r.min = num(e.currentTarget.value))} />
            <span>–</span>
            <input type="number" inputmode="decimal" aria-label="To" placeholder={a.max === null ? 'to' : String(Math.ceil(a.max))} value={r.max ?? ''} oninput={(e) => (r.max = num(e.currentTarget.value))} />
            {#if a.known < (axes.find((x) => x.key === 'playlist')?.known ?? a.known)}<span class="muted">{a.known.toLocaleString()} tracks have one</span>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ol>

  {#if adding}
    <div class="adding" role="group" aria-label="Add a condition on">
      {#each axes as a (a.key)}<button class="value" onclick={() => add(a)}>{a.label}</button>{/each}
      <button class="value" aria-label="Close" onclick={() => (adding = false)}><Icon name="close" size={16} /></button>
    </div>
  {:else}
    <button class="addbtn" onclick={() => (adding = true)} disabled={!axes.length}><Icon name="plus" size={18} /><span>Add condition</span></button>
  {/if}

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

  input {
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .conditions {
    display: grid;
    gap: 8px;
    margin: 4px 0 8px;
    padding: 0;
    list-style: none;
  }

  /* A condition: a card in the accent; negated ones in the danger colour's tint. */
  .cond {
    display: grid;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .cond.negated {
    background: color-mix(in srgb, var(--danger, #c33) 10%, transparent);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .neg {
    min-width: 44px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .neg[aria-pressed='true'] {
    border-color: var(--danger, #c33);
    color: var(--danger, #c33);
  }

  .x {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-left: auto;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .values,
  .adding {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .adding {
    margin: 4px 0;
  }

  .value {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .value[aria-pressed='true'] {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--bg));
  }

  .filter {
    min-height: 34px;
  }

  .range {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .range input {
    width: 96px;
  }

  .addbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border: 1px dashed var(--border);
    border-radius: 999px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .preview {
    margin: 18px -12px 0;
    padding: 0;
    list-style: none;
  }

  .muted,
  .note {
    color: var(--muted);
    font-size: 13px;
  }

  .note {
    margin: 4px 0;
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
