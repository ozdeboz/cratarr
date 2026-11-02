<script lang="ts">
  // One way to change files (decision 0092; the owner, 2026-11-01): Health's Changes to files, in its panel. Three
  // views of one look: a plan to review (its records, each with a tick where there are several; under each, its fields
  // as the files hold them, struck, and as Cratarr holds them, with where from; one button with the count), the runs
  // (Now, then by day; a run's meter while it writes), and a run as written (each record with its Undo, what was set
  // aside and why, Undo of all of it at the foot).
  import { albumCoverUrl, getFilePlan, getFileRun, type FilePlan, type FileRecord, type FileRun } from './api';
  import Asking from './Asking.svelte';
  import { asideOf, changeOf, fieldLabel, runGroups, runLine, type SetAside } from './fileChangeRules';
  import { library } from './albums.svelte';
  import { apply, changesView, fileRuns, keptOf, keptRun, loadRuns, openRun, sent, undoRun } from './fileRuns.svelte';
  import { plural } from './insights.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import SaveBar from './settings/SaveBar.svelte';
  import Icon from './Icon.svelte';
  import Notice from './Notice.svelte';

  let { thing }: { thing: string | null } = $props();
  const v = $derived(changesView(thing));

  $effect(() => loadRuns());

  // A plan: every record ticked to begin with; a long one shown a part at a time.
  const STEP = 100;
  let plan = $state<FilePlan | null>(null);
  let failed = $state<string | null>(null);
  let ticks = $state(new Set<string>());
  let showing = $state(STEP);
  let writing = $state(false);
  /** A failed plan or run read again (its Notice's Try again). */
  let readAgain = $state(0);
  $effect(() => {
    if (v.view !== 'plan') return;
    const { kind, album } = v;
    void readAgain;
    plan = null;
    failed = null;
    showing = STEP;
    let gone = false;
    getFilePlan(kind, album ? [album] : undefined).then(
      (p) => {
        if (gone) return;
        plan = p;
        ticks = new Set(p.records.map((r) => r.album));
      },
      (e) => !gone && (failed = e instanceof Error ? e.message : String(e)),
    );
    return () => (gone = true);
  });
  const many = $derived((plan?.records.length ?? 0) > 1);
  const ticked = $derived(plan ? plan.records.filter((r) => ticks.has(r.album)) : []);
  const filesTicked = $derived(ticked.reduce((n, r) => n + r.files, 0));
  function tick(album: string) {
    const next = new Set(ticks);
    if (!next.delete(album)) next.add(album);
    ticks = next;
  }
  async function write() {
    if (v.view !== 'plan' || !ticked.length) return;
    writing = true;
    await apply(v.kind, ticked);
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
        {@const on = ticks.has(r.album)}
        <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Tick ${r.title}`} onclick={() => tick(r.album)}
          ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
        >
      {:else if end === 'undo' && run}
        <button onclick={() => run && void undoRun(run.run.id, [r.album])}>Undo</button>
      {/if}
    </ReleaseRow>
    <dl class="fields">
      {#each r.fields as f, i (i)}
        {@const c = changeOf(f)}
        <dt>{f.track ?? fieldLabel(f.field)}</dt>
        <dd>
          {#if f.track}<span class="what">{fieldLabel(f.field)}</span>{/if}
          {#each c.gone as g (g)}<s>{g}</s>{/each}{#if c.kind === 'set'}<span class="to">→</span>{:else if c.kind === 'add'}<span class="to">+</span>{/if}{#each c.came as n (n)}<b>{n}</b>{/each}
          <span class="from">{f.from}</span>
        </dd>
      {/each}
    </dl>
  </div>
{/snippet}

{#if v.view === 'plan'}
  {#if failed}
    <Notice text={failed} onretry={() => readAgain++} />
  {:else if !plan}
    <Asking />
  {:else if !plan.records.length}
    <p class="muted">Its files agree with Cratarr.</p>
  {:else}
    <p class="lead">{plan.title} · {plural(plan.records.length, 'record')}</p>
    {#if many}
      <Group>
        <Item name="All of them" toggle={{ checked: ticked.length === plan.records.length, onchange: (on) => (ticks = new Set(on ? plan!.records.map((r) => r.album) : [])) }} />
      </Group>
    {/if}
    <Group>
      {#each plan.records.slice(0, showing) as r (r.album)}{@render record(r, plural(r.files, 'file'), many ? 'tick' : null)}{/each}
      {#if plan.records.length > showing}<Item name={`${(plan.records.length - showing).toLocaleString()} more`} onclick={() => (showing += STEP)} />{/if}
    </Group>
    <SaveBar
      label={many ? `${ticked.length.toLocaleString()} of ${plan.records.length.toLocaleString()} ticked · ${plural(filesTicked, 'file')}` : plural(filesTicked, 'file')}
      action={many ? `Write ${ticked.length.toLocaleString()}` : 'Write'}
      saving={writing}
      canSave={ticked.length > 0}
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
</style>
