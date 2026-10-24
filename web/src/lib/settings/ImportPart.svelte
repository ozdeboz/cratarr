<script lang="ts">
  // Import (decision 0067, laid out as Connections): what you did elsewhere into Cratarr. A row per import (each server
  // Cratarr knows, each Subsonic server imported from, and ListenBrainz), its state in a line; each opens its own page
  // (`thing`: `server:<id>` or `listenbrainz`): where from and into whom, Preview, Import, and Undo once it has run. A
  // server brings plays, favourites, ratings and added dates; ListenBrainz every listen it kept, into the history of the
  // account signed in. Add import: a Subsonic server (Navidrome, gonic, Airsonic, …) by its address and login, found in
  // the library by name; its password is asked for each run and kept nowhere. Acts at once; a server's import is
  // followed while it runs.
  import {
    getLastFm,
    importFile,
    importLastFm,
    importListenBrainz,
    previewLastFm,
    previewListenBrainz,
    undoFileImport,
    undoImport,
    undoLastFm,
    undoListenBrainz,
    undo as undoTokens,
    type FileBrought,
    type ImportTally,
    type LastFmImported,
  } from '../api';
  import { backFromThing, openThing } from '../panel.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import FormSheet from './FormSheet.svelte';
  import { me } from '../me.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { ed, IMPORT_WHAT } from './editor.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  const LISTENBRAINZ = 'listenbrainz';
  const LASTFM = 'lastfm';
  const FILES = 'files';
  const FILES_MORE =
    "A listening history exported elsewhere goes into your history: ListenBrainz's export (its .zip), Spotify's (the JSON files of your extended streaming history, or of your account data; plays under 30 seconds left out) or Last.fm's as export tools write it (CSV: artist, album, track, date). A playlist becomes a new playlist of the tracks found: .m3u, .m3u8, .xspf, or Exportify's CSV of a Spotify playlist.\n\nPreview first; nothing of the file is kept.";
  const FROM_NAMES = { listenbrainz: 'ListenBrainz', spotify: 'Spotify', lastfm: 'Last.fm' } as const;
  const FM_MORE =
    "Your Last.fm scrobbles into your listening history (for your taste and your week, not the play counts), and your loved tracks as favourites. A scrobble already in your history from elsewhere is not added twice; run again, it brings only what is new.\n\nLast.fm asks every app for an API key: make one at last.fm/api/account/create (any name will do) and give it here once.";
  const SERVER = 'server:';
  const SERVER_MORE = 'Plays, favourites, ratings and added dates from a server, into an account. Run again, it brings only what changed.';
  const LB_MORE =
    "Every listen ListenBrainz kept for a user, into your listening history: they count for your taste and your week, not for the play counts. Run again, it brings only new listens.\n\nPreview first shows what would come in and what isn't in your library.";

  const serverId = $derived(thing?.startsWith(SERVER) ? thing.slice(SERVER.length) : null);
  const server = $derived(serverId ? (ed.imp.sources.find((s) => s.id === serverId) ?? null) : null);
  nameThing(() => (thing === LISTENBRAINZ ? 'ListenBrainz' : thing === LASTFM ? 'Last.fm' : thing === FILES ? 'Files' : serverId ? ed.serverName(serverId) : null));
  // A server gone (or never there): back to the list.
  $effect(() => {
    if (serverId && ed.imp.known && !server) backFromThing('import');
  });

  // While a server's import runs, its progress.
  $effect(() => {
    if (!ed.importing) return;
    const t = setInterval(ed.followImport, 2000);
    return () => clearInterval(t);
  });

  let choosing = $state<{ source: string; which: 'user' | 'account' } | null>(null);

  // ListenBrainz: whose listens; the last preview or import, and its undo.
  let lbUser = $state('');
  let naming = $state(false);
  type LbRun = { busy: 'preview' | 'import' | 'undo' | null; tally: ImportTally | null; previewed: boolean; removed: number | null; error: string | null };
  let lb = $state<LbRun>({ busy: null, tally: null, previewed: false, removed: null, error: null });
  let undoing = $state(false);
  async function runLb(what: 'preview' | 'import') {
    lb = { busy: what, tally: null, previewed: false, removed: null, error: null };
    try {
      const tally = what === 'preview' ? await previewListenBrainz(lbUser) : await importListenBrainz(lbUser);
      lb = { busy: null, tally, previewed: what === 'preview', removed: null, error: null };
    } catch (e) {
      lb = { busy: null, tally: null, previewed: false, removed: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
  async function undoLb() {
    lb = { ...lb, busy: 'undo', error: null };
    try {
      lb = { busy: null, tally: null, previewed: false, removed: (await undoListenBrainz()).removed, error: null };
    } catch (e) {
      lb = { ...lb, busy: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
  const n = (x: number) => x.toLocaleString();
  const tallyLine = (t: ImportTally, previewed: boolean) =>
    `${n(t.found)} of ${n(t.items)} found · ${n(t.new)} ${previewed ? 'would be new' : 'new'}`;
  const lbLine = $derived(
    lb.busy ? null : lb.error ? lb.error : lb.tally ? tallyLine(lb.tally, lb.previewed) : lb.removed !== null ? `${n(lb.removed)} listens taken out` : null,
  );

  // Add import: a Subsonic server by its address and login (decision 0067), opened on its own page.
  let adding = $state(false);
  let addUrl = $state('');
  let addUser = $state('');
  const addError = $derived(addUrl.trim() && !/^https?:\/\/./.test(addUrl.trim()) ? 'The address starts with http:// or https://' : null);
  function addSubsonic() {
    const id = `${SUBSONIC}${addUrl.trim().replace(/\/+$/, '')}`;
    const user = addUser.trim();
    if (!ed.imp.sources.some((s) => s.id === id)) ed.imp.sources = [...ed.imp.sources, { id, users: [{ id: user, name: user }], password: true }];
    ed.imp.choice[id] = {
      user,
      account: ed.imp.accounts.includes(me.name ?? '') ? (me.name ?? '') : (ed.imp.accounts[0] ?? ''),
      what: { plays: true, favorites: true, ratings: true, added: true },
    };
    adding = false;
    addUrl = '';
    addUser = '';
    openThing('import', SERVER + id);
  }
  // A Subsonic server's password: for this page's runs only.
  const SUBSONIC = 'subsonic:';
  let passwords = $state<Record<string, string>>({});
  let askingPassword = $state<string | null>(null);

  // A server's import undone (its From and Into as chosen): what it took back.
  let undoingServer = $state<string | null>(null);
  let serverUndone = $state<{ source: string; line: string | null; error: string | null } | null>(null);
  async function undoServer(source: string) {
    const c = ed.imp.choice[source];
    if (!c?.user || !c.account) return;
    try {
      const u = (await undoImport(source, c.user, c.account)).undone;
      const line = u.items
        ? `${n(u.plays)} plays, ${n(u.unstarred)} favourites and ${n(u.unrated)} ratings taken back`
        : 'Nothing was imported from here into that account';
      serverUndone = { source, line, error: null };
    } catch (e) {
      serverUndone = { source, line: null, error: e instanceof Error ? e.message : String(e) };
    }
  }

  // Last.fm: whose scrobbles and the API key (kept on the server once given; never shown again).
  let fmUser = $state('');
  let fmKeyKept = $state(false);
  let fmKey = $state('');
  let fmNaming = $state<'user' | 'key' | null>(null);
  let fmUndoing = $state(false);
  type FmRun = { busy: 'preview' | 'import' | 'undo' | null; done: LastFmImported | null; previewed: boolean; undone: string | null; error: string | null };
  let fm = $state<FmRun>({ busy: null, done: null, previewed: false, undone: null, error: null });
  getLastFm().then(
    (k) => {
      fmUser ||= k.user ?? '';
      fmKeyKept = k.key;
    },
    () => {},
  );
  async function runFm(what: 'preview' | 'import') {
    fm = { busy: what, done: null, previewed: false, undone: null, error: null };
    try {
      const key = fmKey.trim() || undefined;
      const done = what === 'preview' ? await previewLastFm(fmUser, key) : await importLastFm(fmUser, key);
      if (key) {
        fmKeyKept = true;
        fmKey = '';
      }
      fm = { busy: null, done, previewed: what === 'preview', undone: null, error: null };
    } catch (e) {
      fm = { busy: null, done: null, previewed: false, undone: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
  async function undoFm() {
    fm = { ...fm, busy: 'undo', error: null };
    try {
      const u = await undoLastFm(fmUser);
      fm = { busy: null, done: null, previewed: false, undone: `${n(u.scrobbles)} scrobbles and ${n(u.unstarred)} favourites taken back`, error: null };
    } catch (e) {
      fm = { ...fm, busy: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
  const fmLine = $derived(
    fm.busy
      ? null
      : fm.error
        ? fm.error
        : fm.done
          ? `Scrobbles: ${tallyLine(fm.done.scrobbles, fm.previewed)} · Loved: ${n(fm.done.loved.found)} of ${n(fm.done.loved.items)} found`
          : fm.undone,
  );
  const fmReady = $derived(!!fmUser.trim() && (!!fmKey.trim() || fmKeyKept));

  // Files: chosen here, read by the server one at a time, kept nowhere. Each its result; a playlist made can be undone,
  // a history's listens are taken back by their kind.
  let picker = $state<HTMLInputElement>();
  let files = $state<File[]>([]);
  type FileRun = { file: string; done: FileBrought | null; error: string | null };
  let fileRuns = $state<FileRun[]>([]);
  let filesBusy = $state<'preview' | 'import' | null>(null);
  let filesPreviewed = $state(false);
  let undoingFrom = $state<string | null>(null);
  let filesUndone = $state<string | null>(null);
  async function runFiles(what: 'preview' | 'import') {
    filesBusy = what;
    filesUndone = null;
    fileRuns = [];
    for (const f of files) {
      try {
        fileRuns = [...fileRuns, { file: f.name, done: await importFile(f, what === 'preview'), error: null }];
      } catch (e) {
        fileRuns = [...fileRuns, { file: f.name, done: null, error: e instanceof Error ? e.message : String(e) }];
      }
    }
    filesPreviewed = what === 'preview';
    filesBusy = null;
  }
  const fileLine = (r: FileRun) => {
    if (r.error || !r.done) return r.error ?? '';
    const d = r.done;
    return d.kind === 'history'
      ? `${FROM_NAMES[d.from]} history · ${tallyLine(d.tally, filesPreviewed)}`
      : `Playlist ${filesPreviewed ? 'would be' : ''} "${d.name}" · ${n(d.tally.found)} of ${n(d.tally.items)} tracks found`;
  };
  async function undoPlaylist(r: FileRun) {
    if (r.done?.kind !== 'playlist' || !r.done.undo) return;
    try {
      await undoTokens([r.done.undo]);
      fileRuns = fileRuns.map((x) => (x === r ? { ...x, done: null, error: null } : x));
      filesUndone = `Playlist "${r.done.name}" taken back`;
    } catch (e) {
      filesUndone = e instanceof Error ? e.message : String(e);
    }
  }
  async function undoFrom(from: string) {
    try {
      const u = await undoFileImport(from);
      filesUndone = `${n(u.removed)} listens from ${FROM_NAMES[from as keyof typeof FROM_NAMES] ?? from} files taken back`;
    } catch (e) {
      filesUndone = e instanceof Error ? e.message : String(e);
    }
  }

  // A server's import: its state in a line.
  function serverLine(id: string): { lead: string | null; line: string } {
    const job = ed.imp.job?.source === id ? ed.imp.job : null;
    if (!job) return { lead: null, line: 'Not run yet' };
    if (job.finished === null) return { lead: 'off', line: 'Importing' };
    if (job.error) return { lead: 'bad', line: job.error };
    return { lead: 'on', line: `${job.preview ? 'Previewed' : 'Imported'} · ${ed.importLine(job)}` };
  }
</script>

{#if thing === LISTENBRAINZ}
  <Group title="From" more={LB_MORE}>
    <Item name="ListenBrainz user" value={lbUser || 'Choose…'} onclick={() => (naming = true)} />
  </Group>
  <Group>
    <Item
      lead={lb.busy ? 'off' : lb.error ? 'bad' : lb.tally && !lb.previewed ? 'on' : null}
      name={lb.busy === 'preview' ? 'Previewing' : lb.busy === 'import' ? 'Importing' : lb.busy === 'undo' ? 'Taking out' : lb.error ? 'Not imported' : lb.previewed ? 'Previewed' : lb.tally ? 'Imported' : lb.removed !== null ? 'Undone' : 'Not run yet'}
      line={lbLine}
      tone={lb.error ? 'bad' : null}
      wrap
    >
      <button disabled={!!lb.busy || !lbUser.trim()} onclick={() => runLb('preview')}>Preview</button>
      <button class="primary" disabled={!!lb.busy || !lbUser.trim()} onclick={() => runLb('import')}>Import</button>
    </Item>
  </Group>
  {#if lb.tally?.missing.length}
    {@const t = lb.tally}
    <Group title={`Not in your library (${n(t.missing_tracks)})`}>
      {#each t.missing as m (`${m.artist}\n${m.title}`)}
        <Item name={m.title} line={`${m.artist} · ${n(m.count)} ${m.count === 1 ? 'listen' : 'listens'}`} />
      {/each}
      {#if t.missing_tracks > t.missing.length}<Item name={`And ${n(t.missing_tracks - t.missing.length)} more`} />{/if}
    </Group>
  {/if}
  <Group>
    <Item name="Undo the import" kind="danger" onclick={() => (undoing = true)} />
  </Group>
  {#if naming}
    <ValueSheet title="ListenBrainz user" value={lbUser} allowEmpty={false} onsave={(v) => {
        lbUser = v.trim();
      }} onclose={() => (naming = false)} />
  {/if}
  {#if undoing}
    <ConfirmSheet
      title="Undo the ListenBrainz import?"
      text="Every listen imported from ListenBrainz leaves your history. Your play counts stay as they are."
      label="Undo"
      onconfirm={undoLb}
      onclose={() => (undoing = false)}
    />
  {/if}
{:else if thing === LASTFM}
  <Group title="From" more={FM_MORE}>
    <Item name="Last.fm user" value={fmUser || 'Choose…'} onclick={() => (fmNaming = 'user')} />
    <Item
      name="API key"
      line={fmKey ? 'Given' : fmKeyKept ? 'Kept' : 'Needed once: last.fm/api/account/create'}
      tone={fmKey || fmKeyKept ? null : 'warn'}
      onclick={() => (fmNaming = 'key')}
    />
  </Group>
  <Group>
    <Item
      lead={fm.busy ? 'off' : fm.error ? 'bad' : fm.done && !fm.previewed ? 'on' : null}
      name={fm.busy === 'preview'
        ? 'Previewing'
        : fm.busy === 'import'
          ? 'Importing'
          : fm.busy === 'undo'
            ? 'Taking back'
            : fm.error
              ? 'Not imported'
              : fm.previewed
                ? 'Previewed'
                : fm.done
                  ? 'Imported'
                  : fm.undone
                    ? 'Undone'
                    : 'Not run yet'}
      line={fmLine}
      tone={fm.error ? 'bad' : null}
      wrap
    >
      <button disabled={!!fm.busy || !fmReady} onclick={() => runFm('preview')}>Preview</button>
      <button class="primary" disabled={!!fm.busy || !fmReady} onclick={() => runFm('import')}>Import</button>
    </Item>
  </Group>
  {#if fm.done?.scrobbles.missing.length}
    {@const t = fm.done.scrobbles}
    <Group title={`Not in your library (${n(t.missing_tracks)})`}>
      {#each t.missing as m (`${m.artist}|${m.title}`)}
        <Item name={m.title} line={`${m.artist} · ${n(m.count)} ${m.count === 1 ? 'scrobble' : 'scrobbles'}`} />
      {/each}
      {#if t.missing_tracks > t.missing.length}<Item name={`And ${n(t.missing_tracks - t.missing.length)} more`} />{/if}
    </Group>
  {/if}
  <Group>
    <Item name="Undo the import" kind="danger" disabled={!fmUser.trim()} onclick={() => (fmUndoing = true)} />
  </Group>
  {#if fmNaming}
    {@const which = fmNaming}
    <ValueSheet
      title={which === 'user' ? 'Last.fm user' : 'API key'}
      value={which === 'user' ? fmUser : ''}
      help={which === 'key' ? 'Made at last.fm/api/account/create; kept by Cratarr for your next imports.' : null}
      allowEmpty={false}
      onsave={(v) => {
        if (which === 'user') fmUser = v.trim();
        else fmKey = v.trim();
      }}
      onclose={() => (fmNaming = null)}
    />
  {/if}
  {#if fmUndoing}
    <ConfirmSheet
      title="Undo the Last.fm import?"
      text="Every scrobble imported from Last.fm leaves your history, and the favourites its loved tracks set go where you haven't changed them since."
      label="Undo"
      onconfirm={undoFm}
      onclose={() => (fmUndoing = false)}
    />
  {/if}
{:else if thing === FILES}
  <Group title="Files" more={FILES_MORE}>
    <Item
      name={files.length ? files.map((f) => f.name).join(', ') : 'Choose files…'}
      line={files.length ? `${files.length} ${files.length === 1 ? 'file' : 'files'}` : null}
      wrap
      onclick={() => picker?.click()}
    />
    <Item lead={filesBusy ? 'off' : null} name={filesBusy === 'preview' ? 'Previewing' : filesBusy === 'import' ? 'Importing' : filesPreviewed ? 'Previewed' : fileRuns.length ? 'Imported' : 'Not run yet'}>
      <button disabled={!!filesBusy || !files.length} onclick={() => runFiles('preview')}>Preview</button>
      <button class="primary" disabled={!!filesBusy || !files.length} onclick={() => runFiles('import')}>Import</button>
    </Item>
  </Group>
  <input class="picker" type="file" multiple accept=".zip,.json,.jsonl,.csv,.m3u,.m3u8,.xspf" bind:this={picker} onchange={(e) => (files = [...(e.currentTarget.files ?? [])])} />
  {#if fileRuns.length}
    <Group title="Results">
      {#each fileRuns as r (r.file)}
        <Item lead={r.error ? 'bad' : 'on'} name={r.file} line={fileLine(r)} tone={r.error ? 'bad' : null} wrap>
          {#if !filesPreviewed && r.done?.kind === 'playlist' && r.done.undo}
            {@const run = r}
            <button onclick={() => undoPlaylist(run)}>Undo</button>
          {/if}
        </Item>
      {/each}
    </Group>
    {#each fileRuns.filter((r) => r.done?.tally.missing.length) as r (r.file)}
      {@const t = r.done!.tally}
      <Group title={`Not in your library: ${r.file} (${n(t.missing_tracks)})`}>
        {#each t.missing.slice(0, 20) as m (`${m.artist}|${m.title}`)}
          <Item name={m.title} line={`${m.artist} · ${n(m.count)}`} />
        {/each}
        {#if t.missing_tracks > 20}<Item name={`And ${n(t.missing_tracks - 20)} more`} />{/if}
      </Group>
    {/each}
  {/if}
  <Group title="Undo">
    {#if filesUndone}<Item name={filesUndone} wrap />{/if}
    {#each Object.entries(FROM_NAMES) as [from, label] (from)}
      <Item name={`Take back the ${label} listens imported from files`} kind="danger" onclick={() => (undoingFrom = from)} />
    {/each}
  </Group>
  {#if undoingFrom}
    {@const from = undoingFrom}
    <ConfirmSheet
      title={`Undo the ${FROM_NAMES[from as keyof typeof FROM_NAMES]} file imports?`}
      text="Every listen imported from such files leaves your history. Your play counts stay as they are."
      label="Undo"
      onconfirm={() => undoFrom(from)}
      onclose={() => (undoingFrom = null)}
    />
  {/if}
{:else if server}
  {@const src = server}
  {@const c = ed.imp.choice[src.id]}
  {@const job = ed.imp.job?.source === src.id ? ed.imp.job : null}
  {#if src.error}
    <Group><Item lead="bad" name={src.error} /></Group>
  {:else if !ed.sub.accounts?.length}
    <Group><Item name="Create an account under Users first" /></Group>
  {:else if c}
    {@const pw = src.password ? passwords[src.id] : undefined}
    {@const none = !Object.values(c.what).some(Boolean) || !c.user || !c.account || (src.password && !pw)}
    <Group title="From and into" more={SERVER_MORE}>
      {#if src.password}
        <Item name="Server" value={ed.serverName(src.id)} />
        <Item name="User" value={c.user} />
        <Item name="Password" line={pw ? 'Given for this visit' : 'Needed for each run; kept nowhere'} tone={pw ? null : 'warn'} onclick={() => (askingPassword = src.id)} />
      {:else}
        <Item name="From" value={src.users.find((u) => u.id === c.user)?.name ?? 'Choose…'} onclick={() => (choosing = { source: src.id, which: 'user' })} />
      {/if}
      <Item name="Into" value={c.account || 'Choose…'} onclick={() => (choosing = { source: src.id, which: 'account' })} />
    </Group>
    <Group title="What">
      {#each IMPORT_WHAT as w (w.key)}
        <Item name={w.label} toggle={{ checked: c.what[w.key], onchange: (on) => (c.what[w.key] = on) }} />
      {/each}
    </Group>
    <Group>
      {#if job && job.finished === null}
        {@const st = ed.stepLine(job)}
        <Item lead="off" name="Importing" line={`${st.label}${st.value ? ` · ${Math.round(st.value * 100)}%` : ''}`} />
      {:else}
        <Item
          lead={job ? (job.error ? 'bad' : 'on') : null}
          name={job ? (job.error ? 'Not imported' : job.preview ? 'Previewed' : 'Imported') : 'Not run yet'}
          line={job ? (job.error ?? ed.importLine(job)) : null}
          tone={job?.error ? 'bad' : null}
          wrap
        >
          <button disabled={ed.importing || none} onclick={() => ed.runImport(src.id, true, pw)}>Preview</button>
          <button class="primary" disabled={ed.importing || none} onclick={() => ed.runImport(src.id, false, pw)}>Import</button>
        </Item>
      {/if}
      {#if job?.note}<Item lead="warn" name={job.note} />{/if}
    </Group>
    {#if askingPassword === src.id}
      <ValueSheet
        title="Password"
        type="password"
        value=""
        help={`For ${c.user} on ${ed.serverName(src.id)}: used for this visit's runs, kept nowhere.`}
        allowEmpty={false}
        onsave={(v) => {
          passwords[src.id] = v;
        }}
        onclose={() => (askingPassword = null)}
      />
    {/if}
    <Group>
      {#if serverUndone?.source === src.id}
        {@const u = serverUndone}
        <Item lead={u.error ? 'bad' : 'on'} name={u.error ? 'Not undone' : 'Undone'} line={u.error ?? u.line} tone={u.error ? 'bad' : null} wrap />
      {/if}
      <Item name="Undo the import" kind="danger" disabled={ed.importing || !c.user || !c.account} onclick={() => (undoingServer = src.id)} />
    </Group>
    {#if undoingServer === src.id}
      <ConfirmSheet
        title="Undo the import?"
        text={`The plays it added go, and the favourites and ratings it set where you haven't changed them since, for ${c.account}. Last played and added dates stay.`}
        label="Undo"
        onconfirm={() => undoServer(src.id)}
        onclose={() => (undoingServer = null)}
      />
    {/if}
  {/if}
{:else}
  <Group title="Imports" more="What you did elsewhere, into Cratarr: preview first, then import; run again, it brings only what is new.">
    {#each ed.imp.sources as src (src.id)}
      {@const s = serverLine(src.id)}
      <Item lead={src.error ? 'bad' : s.lead} name={`From ${ed.serverName(src.id)}`} line={src.error ?? s.line} tone={src.error || s.lead === 'bad' ? 'bad' : null} onclick={() => openThing('import', SERVER + src.id)} />
    {/each}
    <Item
      lead={lb.tally && !lb.previewed ? 'on' : lb.error ? 'bad' : null}
      name="From ListenBrainz"
      line={lbLine ?? 'Listens into your history'}
      tone={lb.error ? 'bad' : null}
      onclick={() => openThing('import', LISTENBRAINZ)}
    />
    <Item
      lead={fm.done && !fm.previewed ? 'on' : fm.error ? 'bad' : null}
      name="From Last.fm"
      line={fmLine ?? 'Scrobbles into your history, loved tracks as favourites'}
      tone={fm.error ? 'bad' : null}
      onclick={() => openThing('import', LASTFM)}
    />
    <Item name="From files" line="A history export, or a playlist" onclick={() => openThing('import', FILES)} />
    <Item name="Add import" kind="add" onclick={() => (adding = true)} />
  </Group>
  {#if adding}
    <FormSheet
      title="From a Subsonic server"
      submitLabel="Add"
      canSubmit={!!addUrl.trim() && !!addUser.trim() && !addError}
      error={addError}
      onsubmit={addSubsonic}
      onclose={() => (adding = false)}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <label>Server address<input type="url" bind:value={addUrl} placeholder="https://music.example.org" autocomplete="off" spellcheck="false" autofocus /></label>
      <label>User<input type="text" bind:value={addUser} autocomplete="off" spellcheck="false" /></label>
    </FormSheet>
  {/if}
{/if}

{#if choosing}
  {@const ch = choosing}
  {@const src = ed.imp.sources.find((x) => x.id === ch.source)}
  {@const c = ed.imp.choice[ch.source]}
  {#if src && c}
    <ChoiceSheet
      title={ch.which === 'user' ? 'From' : 'Into'}
      options={ch.which === 'user' ? src.users.map((u) => ({ value: u.id, label: u.name })) : ed.imp.accounts.map((a) => ({ value: a, label: a }))}
      value={ch.which === 'user' ? c.user : c.account}
      onchange={(v) => (ch.which === 'user' ? (c.user = v) : (c.account = v))}
      onclose={() => (choosing = null)}
    />
  {/if}
{/if}

<style>
  .picker {
    display: none;
  }
</style>
