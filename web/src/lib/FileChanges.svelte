<script lang="ts">
  // One way to change files (decision 0092; the owner, 2026-11-01): Health's Changes to files, in its panel. Three
  // views of one look: a plan to review (its records, each with a tick where there are several; under each, its fields
  // as the files hold them, struck, and as Cratarr holds them, with where from; one button with the count), the runs
  // (Now, then by day; a run's meter while it writes), and a run as written (each record with its Undo, what was set
  // aside and why, Undo of all of it at the foot).
  import ChoiceMark from './ChoiceMark.svelte';
  import { albumCoverUrl, getFilePlan, getFileRun, type FileChosen, type FileField, type FilePlan, type FileRecord, type FileRun } from './api';
  import GenrePick from './GenrePick.svelte';
  import Asking from './Asking.svelte';
  import { asideOf, changeOf, fieldLabel, rowLabel, runGroups, runLine, type SetAside } from './fileChangeRules';
  import { library, readLibrary } from './albums.svelte';
  import { apply, changesView, fileRuns, keptOf, keptRun, loadRuns, openRun, sent, undoRun } from './fileRuns.svelte';
  import { plural } from './insights.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import { OWN } from './sources.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import SaveBar from './settings/SaveBar.svelte';
  import Icon from './Icon.svelte';
  import Notice from './Notice.svelte';

  let { thing }: { thing: string | null } = $props();
  const v = $derived(changesView(thing));

  $effect(() => loadRuns());

  // A row's library goes by the name the Library's list gives it, read here if nothing has yet: a review can be the
  // first thing opened.
  if (!library.view) readLibrary(false).catch(() => {});
  const libraries = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);

  // A plan: every record ticked to begin with; a long one shown a part at a time.
  const STEP = 100;
  let plan = $state<FilePlan | null>(null);
  let failed = $state<string | null>(null);
  let ticks = $state(new Set<string>());
  let showing = $state(STEP);
  let writing = $state(false);
  /** A failed plan or run read again (its Notice's Try again). */
  let readAgain = $state(0);
  // A fix plan (`fix:`, `intake:`; decision 0092's writers' map, 0.14): each field ticked as sure as it is (certain ones
  // ticked, likely ones not: the owner's choice 2), a record ticked while any of its fields is; a genre's words picked
  // among those found. Asked again while it is still being made (a match looked for), what was ticked kept.
  const keyOf = (album: string, f: FileField) => `${album}\u0000${f.field}`;
  let fieldTicks = $state(new Set<string>());
  let genres = $state<Record<string, string[]>>({});
  let seen = new Set<string>();
  let shownKind = '';
  const byField = $derived(!!plan?.records.some((r) => r.fields.some((f) => f.certainty)));
  $effect(() => {
    if (v.view !== 'plan') return;
    const { kind, album } = v;
    void readAgain;
    const thing = `${kind}/${album ?? ''}`;
    if (thing !== shownKind) {
      shownKind = thing;
      plan = null;
      showing = STEP;
      seen = new Set();
      genres = {};
    }
    failed = null;
    let gone = false;
    let again: ReturnType<typeof setTimeout> | undefined;
    getFilePlan(kind, album ? [album] : undefined).then(
      (p) => {
        if (gone) return;
        const fresh = (k: string) => !seen.has(k) && (seen.add(k), true);
        const records = new Set(ticks);
        const fields = new Set(fieldTicks);
        for (const r of p.records) {
          if (fresh(r.album)) records.add(r.album);
          for (const f of r.fields) if (fresh(keyOf(r.album, f)) && f.certainty !== 'likely') fields.add(keyOf(r.album, f));
        }
        plan = p;
        ticks = records;
        fieldTicks = fields;
        if (p.making) again = setTimeout(() => readAgain++, 2000);
      },
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => {
      gone = true;
      clearTimeout(again);
    };
  });
  const many = $derived((plan?.records.length ?? 0) > 1);
  const fieldsOn = (r: FileRecord) => r.fields.filter((f) => fieldTicks.has(keyOf(r.album, f)));
  const ticked = $derived(plan ? plan.records.filter((r) => (byField ? fieldsOn(r).length > 0 : ticks.has(r.album))) : []);
  const filesTicked = $derived(ticked.reduce((n, r) => n + r.files, 0));
  const isTicked = (r: FileRecord) => (byField ? fieldsOn(r).length === r.fields.length : ticks.has(r.album));
  function tick(r: FileRecord) {
    if (byField) return tickAll([r], !isTicked(r));
    const next = new Set(ticks);
    if (!next.delete(r.album)) next.add(r.album);
    ticks = next;
  }
  function tickAll(records: FileRecord[], on: boolean) {
    if (!byField) return (ticks = new Set(on ? records.map((r) => r.album) : []));
    const next = new Set(fieldTicks);
    for (const r of records)
      for (const f of r.fields) {
        if (on) next.add(keyOf(r.album, f));
        else next.delete(keyOf(r.album, f));
      }
    fieldTicks = next;
  }
  function tickField(album: string, f: FileField) {
    const next = new Set(fieldTicks);
    if (!next.delete(keyOf(album, f))) next.add(keyOf(album, f));
    fieldTicks = next;
  }
  const pickedOf = (album: string, f: FileField) => genres[album] ?? f.after;
  /** A genre ticked with none of its words picked: nothing to write it as. */
  const nonePicked = $derived(ticked.some((r) => fieldsOn(r).some((f) => f.options?.length && !pickedOf(r.album, f).length)));
  async function write() {
    if (v.view !== 'plan' || !ticked.length) return;
    writing = true;
    const chosen: FileChosen | undefined = byField
      ? {
          fields: Object.fromEntries(ticked.map((r) => [r.album, [...new Set(fieldsOn(r).map((f) => f.field))]])),
          genres: Object.fromEntries(ticked.flatMap((r) => (r.album in genres && fieldsOn(r).some((f) => f.options?.length) ? [[r.album, genres[r.album]]] : []))),
        }
      : undefined;
    await apply(v.kind, ticked, chosen);
    writing = false;
  }

  // A run under way (`l…`): its status as its news comes, the records the sheet sent; once kept, its page as written in
  // its place.
  const live = $derived(v.view === 'run' && v.id.startsWith('l') ? (fileRuns.list?.find((r) => r.id === v.id) ?? null) : null);
  $effect(() => {
    if (v.view !== 'run' || !v.id.startsWith('l')) return;
    const kept = keptOf(v.id);
    if (kept) keptRun(kept.id);
  });
  // A run as written, read again as its news comes (an Undo, the run ending).
  let run = $state<{ run: FileRun; kind: string; title: string; records: FileRecord[] } | null>(null);
  const news = $derived(v.view === 'run' ? JSON.stringify(fileRuns.list?.find((r) => r.id === v.id) ?? null) : '');
  $effect(() => {
    if (v.view !== 'run' || v.id.startsWith('l')) return;
    const id = v.id;
    void news;
    void readAgain;
    failed = null;
    let gone = false;
    getFileRun(id).then(
      (r) => !gone && (run = r),
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => (gone = true);
  });
  /** A record set aside by its name: the server's, the run's, else the library's; never its id. */
  const titleOf = (x: SetAside) => {
    const r = run?.records.find((y) => y.album === x.album);
    const a = !r && !x.title ? library.view?.sources.flatMap((s) => s.albums).find((y) => y.id === x.album) : null;
    const title = x.title ?? r?.title ?? a?.title;
    const artist = x.artist ?? r?.artist ?? a?.artist;
    return title ? (artist ? `${artist} – ${title}` : title) : 'A record';
  };
</script>

<!-- A record and its fields: as the files hold them, struck, and as Cratarr holds them, with where from. -->
{#snippet record(r: FileRecord, line: string, end: 'tick' | 'undo' | null)}
  <div class="rec">
    <ReleaseRow cover={albumCoverUrl('cratarr', r.album)} title={`${r.artist} – ${r.title}`} {line}>
      {#if end === 'tick'}
        {@const on = isTicked(r)}
        <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Tick ${r.title}`} onclick={() => tick(r)}
          ><ChoiceMark kind="several" on={on} /></button
        >
      {:else if end === 'undo' && run}
        <button onclick={() => run && void undoRun(run.run.id, [r.album])}>Undo</button>
      {/if}
    </ReleaseRow>
    <dl class="fields">
      {#each r.fields as f, i (i)}
        {@const c = changeOf(f)}
        {@const fieldTick = end === 'tick' && byField}
        <dt>
          {#if fieldTick}
            {@const on = fieldTicks.has(keyOf(r.album, f))}
            <button class="tick small" class:on role="checkbox" aria-checked={on} aria-label={`Tick ${fieldLabel(f.field)}`} onclick={() => tickField(r.album, f)}
              ><ChoiceMark kind="several" on={on} /></button
            >
          {/if}{rowLabel(f, libraries)}
        </dt>
        <dd>
          {#if fieldTick && f.options?.length}
            <span class="pick"
              ><GenrePick
                fix={{ from: f.before.join('; '), candidates: f.options.map((o) => ({ word: o.value, kind: o.kind ?? 'genre', source: o.from, certainty: o.certainty })) }}
                bind:picked={() => pickedOf(r.album, f), (w) => (genres[r.album] = w)}
              /></span
            >
          {:else if f.field === 'cover' && f.after[0]?.startsWith('http')}
            <img class="pic" src={f.after[0]} alt="" />
          {:else}
            {#if f.track}<span class="what">{fieldLabel(f.field)}</span>{/if}
            {#each c.gone as g (g)}<s>{g}</s>{/each}{#if c.kind === 'set'}<span class="to">→</span>{:else if c.kind === 'add'}<span class="to">+</span>{/if}{#each c.came as n (n)}<b>{n}</b>{/each}
          {/if}
          <span class="from">{f.from}{#if f.certainty === 'likely'} · likely{/if}</span>
        </dd>
      {/each}
    </dl>
  </div>
{/snippet}

{#if v.view === 'plan'}
  {#if failed}
    <Notice text={failed} onretry={() => readAgain++} />
  {:else if !plan || (plan.making && !plan.records.length)}
    <Asking />
  {:else if !plan.records.length}
    <p class="muted">Its files agree with Cratarr.</p>
  {:else}
    <p class="lead">{plan.title} · {plural(plan.records.length, 'record')}{#if plan.making} · looking for more{/if}</p>
    {#if many}
      <Group>
        <Item name="All of them" toggle={{ checked: plan.records.every(isTicked), onchange: (on) => tickAll(plan!.records, on) }} />
      </Group>
    {/if}
    <Group>
      {#each plan.records.slice(0, showing) as r (r.album)}{@render record(r, plural(r.files, 'file'), many || byField ? 'tick' : null)}{/each}
      {#if plan.records.length > showing}<Item name={`${(plan.records.length - showing).toLocaleString()} more`} onclick={() => (showing += STEP)} />{/if}
    </Group>
    <SaveBar
      label={many ? `${ticked.length.toLocaleString()} of ${plan.records.length.toLocaleString()} ticked · ${plural(filesTicked, 'file')}` : plural(filesTicked, 'file')}
      action={many ? `Write ${ticked.length.toLocaleString()}` : 'Write'}
      saving={writing}
      canSave={ticked.length > 0 && !nonePicked}
      onsave={() => void write()}
    />
  {/if}
{:else if v.view === 'run' && v.id.startsWith('l')}
  <!-- Under way: its meter, what the sheet sent, what is set aside so far. -->
  {#if live}
    <Group>
      <Item name={`${live.title} · ${plural(live.of || sent.records.length, 'record')}`} line={runLine(live)} progress={live.state === 'writing' && live.of ? live.done / live.of : live.state === 'waiting' ? 0 : null} />
    </Group>
    {#if sent.id === live.id && sent.records.length}
      <Group>
        {#each sent.records.slice(0, showing) as x (x.album)}{@render record(x, plural(x.files, 'file'), null)}{/each}
        {#if sent.records.length > showing}<Item name={`${(sent.records.length - showing).toLocaleString()} more`} onclick={() => (showing += STEP)} />{/if}
      </Group>
    {/if}
    {#if live.failed.length}
      <Group title="Set aside">
        {#each asideOf(live.failed) as x (x.album)}<Item lead="warn" name={titleOf(x)} line={[x.files ? plural(x.files, 'file') : '', ...x.why].filter(Boolean).join(' · ')} tone="warn" wrap />{/each}
      </Group>
    {/if}
  {:else if fileRuns.error && !fileRuns.list}
    <Notice text={fileRuns.error} onretry={loadRuns} />
  {:else if !fileRuns.list}
    <Asking />
  {:else}
    <p class="muted">This run is no longer under way.</p>
  {/if}
{:else if v.view === 'run'}
  {#if failed}
    <Notice text={failed} onretry={() => readAgain++} />
  {:else if !run}
    <Asking />
  {:else}
    {@const r = run.run}
    <!-- Only what it wrote; a record it wrote nothing of is said under Set aside alone (the dev instance's check,
         2026-11-01: a refused record showed twice, the first time as Undone). -->
    {@const wrote = run.records.filter((x) => (x.written ?? 0) > 0)}
    <p class="lead">{runLine(r) || plural(r.albums, 'record')}</p>
    {#if wrote.length}
      <Group>
        {#each wrote as x (x.album)}{@render record(x, x.undone ? 'Undone' : `${x.written ?? 0} of ${plural(x.files, 'file')} written`, r.undo && !r.undone && !x.undone ? 'undo' : null)}{/each}
      </Group>
    {/if}
    {#if r.failed.length}
      <Group title="Set aside">
        {#each asideOf(r.failed) as x (x.album)}<Item lead="warn" name={titleOf(x)} line={[x.files ? plural(x.files, 'file') : '', ...x.why].filter(Boolean).join(' · ')} tone="warn" wrap />{/each}
      </Group>
    {/if}
    {#if r.undo && !r.undone && wrote.length}
      <SaveBar label={`${plural(r.albums, 'record')} written`} action="Undo all" onsave={() => void undoRun(r.id)} />
    {/if}
  {/if}
{:else if fileRuns.error}
  <Notice text={fileRuns.error} onretry={loadRuns} />
{:else if !fileRuns.list}
  <Asking />
{:else if !fileRuns.list.length}
  <p class="muted">No changes to files this month.</p>
{:else}
  {#each runGroups(fileRuns.list) as g (g.title)}
    <Group title={g.title}>
      {#each g.runs as r (r.id)}
        <Item
          name={`${r.title} · ${plural(r.albums, 'record')}`}
          line={runLine(r)}
          tone={r.state === 'done' && !r.undone && r.failed.length ? 'warn' : null}
          progress={r.state === 'writing' && r.of ? r.done / r.of : null}
          onclick={() => openRun(r.id)}
        />
      {/each}
    </Group>
  {/each}
{/if}

<style>
  .lead,
  .muted {
    margin: 4px 4px 14px;
    color: var(--muted);
    font-size: 14px;
  }

  .rec > :global(.release) {
    border-radius: 0;
  }

  /* Its fields under it, in line with its title. */
  .fields {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 12px;
    margin: 0;
    padding: 0 14px 12px 72px;
    font-size: 14px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 6px;
    margin: 0;
    min-width: 0;
  }

  s,
  .what,
  .to,
  .from {
    color: var(--muted);
  }

  b {
    font-weight: 600;
  }

  .from {
    font-size: 12.5px;
  }

  .from::before {
    content: '· ';
  }

  .tick {
    display: grid;
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

  /* A field's own tick, before its name. */
  .tick.small {
    display: inline-grid;
    width: 28px;
    height: 28px;
    margin: -4px 4px -4px -32px;
    vertical-align: middle;
  }

  .pic {
    width: 96px;
    height: 96px;
    border-radius: 6px;
    object-fit: cover;
  }

  .pick {
    display: grid;
    gap: 4px;
    min-width: 0;
  }
</style>
