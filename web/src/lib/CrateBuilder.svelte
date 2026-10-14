<script lang="ts">
  // The crate builder (decision 0024), the panel's third mode: a crate of one's own made or changed from the library's
  // axes. Its conditions all hold (AND), each can be negated; OR within an axis by choosing several values or a range.
  // The controls come from the server's axis list (kind and domain), so a new axis needs nothing here. While building,
  // the unsaved crate's size and first tracks show below (after 300 ms). The draft is kept in this browser under its
  // own address until Save or Cancel (it survives the player over it, a page change and a reload).
  import {
    coverUrl,
    createCrate,
    deleteCrate,
    getCrate,
    getCrateAxes,
    previewCrate,
    updateCrate,
    type Axis,
    type Condition,
    type PlaylistRow,
  } from './api';
  import { CRATE, crates, loadCrates } from './crates.svelte';
  import Icon from './Icon.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import { closePlaylist, viewPlaylist, panel } from './panel.svelte';
  import { prefs } from './prefs.svelte';
  import { toast } from './toast.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';
  import { withUndo } from './undo.svelte';

  let { id }: { id: string | null } = $props();

  type Draft = { name: string; conditions: Condition[] };
  const draftKey = $derived(`cratarr-crate-draft:${id ?? 'new'}`);
  let draft = $state<Draft | null>(null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // The draft: kept in this browser while open; else the crate as saved, or an empty one.
  $effect(() => {
    const key = draftKey;
    const which = id;
    void panel.again;
    draft = null;
    let kept: Draft | null = null;
    try {
      kept = JSON.parse(localStorage.getItem(key) ?? 'null');
    } catch {
      kept = null;
    }
    if (kept && Array.isArray(kept.conditions)) {
      draft = kept;
      return;
    }
    if (!which) {
      draft = { name: '', conditions: [] };
      return;
    }
    getCrate(which).then(
      (c) => which === id && (draft = { name: c.name, conditions: c.conditions }),
      (e) => (error = e instanceof Error ? e.message : 'Unexpected error'),
    );
  });
  $effect(() => {
    const snapshot = draft && JSON.stringify(draft);
    if (!snapshot) return;
    try {
      localStorage.setItem(draftKey, snapshot);
    } catch {
      /* not kept */
    }
  });
  const forget = () => {
    try {
      localStorage.removeItem(draftKey);
    } catch {
      /* nothing kept */
    }
  };

  // The axes: their kinds and domains in this library.
  let axes = $state<Axis[]>([]);
  $effect(() => {
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      (e) => toast.fail(e, "The crate's axes couldn't be loaded"),
    );
  });
  const axis = (key: string) => axes.find((a) => a.key === key);

  function add(a: Axis) {
    if (!draft) return;
    const value: Condition['value'] = a.kind === 'yes_no' ? { kind: 'yes' } : a.kind === 'range' ? { kind: 'range', min: null, max: null } : { kind: 'any_of', values: [] };
    draft.conditions = [...draft.conditions, { axis: a.key, not: false, value }];
    adding = false;
  }
  let adding = $state(false);
  const remove = (i: number) => draft && (draft.conditions = draft.conditions.filter((_, j) => j !== i));
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
  const key = $derived(draft ? JSON.stringify(draft.conditions) : '');
  $effect(() => {
    const k = key;
    if (!draft) return;
    const conditions = JSON.parse(k) as Condition[];
    const t = setTimeout(() => {
      previewCrate(conditions, 50, prefs.source || undefined).then(
        (p) => k === key && ((preview = p), (previewError = null)),
        (e) => k === key && (previewError = e instanceof Error ? e.message : 'Unexpected error'),
      );
    }, 300);
    return () => clearTimeout(t);
  });

  /** After a crate's undo or redo: the list follows, and a crate that is gone no longer shows in the panel. */
  async function followCrate(crate: string) {
    await loadCrates();
    if (panel.viewed === CRATE + crate && !crates.list?.some((c) => c.id === crate)) closePlaylist();
  }

  async function save() {
    if (!draft || !draft.name.trim() || saving) return;
    saving = true;
    const name = draft.name.trim();
    try {
      const saved = id ? await updateCrate(id, draft.name, draft.conditions) : await createCrate(draft.name, draft.conditions);
      withUndo(id ? `Saved ${name}.` : `Created ${name}.`, [saved.undo], { after: () => followCrate(saved.id) });
      forget();
      await loadCrates();
      viewPlaylist(CRATE + saved.id);
    } catch (e) {
      toast.fail(e, "The crate wasn't saved");
    } finally {
      saving = false;
    }
  }

  function cancel() {
    forget();
    if (id) viewPlaylist(CRATE + id);
    else closePlaylist();
  }

  // Delete (one's own crate): a second tap confirms.
  let confirmDelete = $state(false);
  async function remove_crate() {
    if (!id) return;
    if (!confirmDelete) return void (confirmDelete = true);
    const crate = id;
    const name = draft?.name.trim() || crate;
    try {
      const { undo } = await deleteCrate(crate);
      withUndo(`Deleted ${name}.`, [undo], { after: () => followCrate(crate) });
      forget();
      await loadCrates();
      closePlaylist();
    } catch (e) {
      toast.fail(e, "The crate wasn't deleted");
    }
  }

  const permanentNames = $derived((crates.list ?? []).filter((c) => c.permanent).map((c) => c.name.toLowerCase()));
</script>

<section class="view" aria-label={id ? 'Change the crate' : 'New crate'}>
  <div class="bar">
    <!-- ✕ closes the panel (the draft stays, as when the builder is left); Cancel, against Save, drops the draft. -->
    <button class="icon" aria-label="Close the builder" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
    <span class="end">
      <button class="barbtn iconic" aria-label="Cancel" onclick={cancel} disabled={saving}><Icon name="reset" size={18} /><span>Cancel</span></button>
      <button class="barbtn save iconic" aria-label="Save" onclick={save} disabled={saving || !draft?.name.trim()} aria-busy={saving}><Icon name="arrowDownToLine" size={18} /><span>Save</span></button>
      <PanelPlace />
    </span>
  </div>

  <div class="scroll">
    {#if error}
      <p class="error" role="alert">{error}</p>
    {:else if !draft}
      <p class="empty">Loading…</p>
    {:else}
      <label class="field name">
        <span>Name</span>
        <input type="text" maxlength="100" placeholder="New crate" bind:value={draft.name} disabled={saving} />
      </label>
      {#if permanentNames.includes(draft.name.trim().toLowerCase())}<p class="note warn">That is a permanent crate's name.</p>{/if}

      <ol class="conditions">
        {#each draft.conditions as c, i (i)}
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
          <button class="value" onclick={() => (adding = false)}><Icon name="close" size={16} /></button>
        </div>
      {:else}
        <button class="addbtn" onclick={() => (adding = true)} disabled={!axes.length}><Icon name="plus" size={18} /><span>Add condition</span></button>
      {/if}

      <section class="preview" aria-label="Preview">
        <h3>
          {#if previewError}<span class="error">{previewError}</span>{:else if preview}{preview.total.toLocaleString()} {preview.total === 1 ? 'track' : 'tracks'}{:else}…{/if}
        </h3>
        {#if preview}
          <ol>
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
      </section>

      {#if id}
        <button class="delete" onclick={remove_crate}>{confirmDelete ? 'Tap again to delete' : 'Delete crate'}</button>
      {/if}
    {/if}
  </div>
</section>

<style>
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--surface);
  }

  .bar {
    flex: none;
    display: flex;
    justify-content: space-between;
    padding: 4px 4px 0;
  }

  .end {
    display: flex;
    align-items: center;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .barbtn {
    min-height: 40px;
    margin: 4px 8px;
    padding: 0 14px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  /* Cancel and Save carry an icon beside their label (✕, an arrow down to a line). */
  .barbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* A narrow panel: Cancel and Save show their icons only (the labels stay for screen readers). */
  @container (max-width: 339px) {
    .barbtn.iconic span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }

    .barbtn.iconic {
      padding: 0 10px;
    }
  }

  .barbtn.save {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .barbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 8px 12px calc(32px + var(--mini-h, 0px) + env(safe-area-inset-bottom));
  }

  .field {
    display: grid;
    gap: 4px;
    font-size: 13px;
    color: var(--muted);
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
    margin: 12px 0 8px;
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

  .preview h3 {
    margin: 18px 0 6px;
    font-size: 14px;
    font-weight: 500;
  }

  .preview ol {
    margin: 0;
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

  .note.warn,
  .error {
    color: var(--danger, #c33);
  }

  .empty {
    margin: 16px;
    color: var(--muted);
  }

  .delete {
    display: block;
    margin: 24px 0 8px;
    padding: 10px 14px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--danger, #c33);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
</style>
