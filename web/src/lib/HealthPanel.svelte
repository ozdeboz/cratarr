<script lang="ts">
  // A problem in the panel (the owner, 2026-10-23): one layout for every problem, filled from its data
  // (lib/healthReport's Problem), never laid out per problem: its domain › what it is; how it stands (critical, a
  // warning, to know) and since when, what it means behind its (i); how much, as a meter where there is a whole; the
  // things it is about, one by one, as a group of rows. Laid out as Settings, one level only (0.13; the owner,
  // 2026-10-31, mockup C: "the flow from page to panel to extra panel is crappy"): a row opens in place, one at a time,
  // what can be done with it under it (its missing tracks each with Search, its numbers, its fixes, its files), and where
  // its tracks are looked for one at a time, Work through takes them in turn in one sheet (lib/ReplaceTrackSheet
  // `through`, decision 0080: each still its own choice). Many at once (mockup D; the owner, 2026-10-31: "Bulk search,
  // you approve", 0080 amended): albums ticked, their missing tracks looked for in turn, each found one's best copy
  // chosen, and a review list over the albums where nothing goes in until it is ticked and added (lib/trackIntake).
  // Beside the list on a roomy window (no ✕ there), over it on a phone.
  import { untrack } from 'svelte';
  import { findsTracks, offers } from './capabilities.svelte';
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import Icon from './Icon.svelte';
  import MoreInfo from './MoreInfo.svelte';
  import { albumCoverUrl, undo, wantAlbum, type HealthItem } from './api';
  import { plural } from './insights.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import SelectBar from './SelectBar.svelte';
  import { approveIntake, clearIntake, copyLine, dropIntake, getIntake, startIntake, type Intake, type IntakeItem } from './replaceTrack';
  import { anyOver, approvePayload, copyOf, foundIds, itemEnd, itemLine, itemTone, keptTicks, lookedFor, tickable, underWay } from './trackIntake';
  import { toast } from './toast.svelte';
  import { openFinding } from './health';
  import { health, loadHealth, problems } from './healthReport.svelte';
  import { damagedStops, missingStops, throughKind, throughTotal, type Stop } from './healthThrough';
  import HealthEvents from './HealthEvents.svelte';
  import HealthIncoming from './HealthIncoming.svelte';
  import HealthMissing from './HealthMissing.svelte';
  import HealthRenumber from './HealthRenumber.svelte';
  import HealthFixes from './HealthFixes.svelte';
  import { apply, FIXABLE, healthFixes, readFixes } from './fixes.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import ReplaceTrackSheet from './ReplaceTrackSheet.svelte';
  import { releaseOf, type ReleaseTrack } from './replaceTrack';
  import { me } from './me.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import { matchesSearch } from './pageView.svelte';
  import ContentControls from './ContentControls.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { albumSubject, backOne, closePlaylist, HEALTH, healthThing, isStep, openTab, panel, upToPage, viewOver, viewPlaylist, viewSibling } from './panel.svelte';
  import { byArea } from './healthThrough';
  import { AREAS } from './health';
  import { planTitle } from './fileChangeRules';
  import type { Level, Siblings } from './Crumbs.svelte';
  import { groupsOf, landingOf, remembered } from './groupCrumb';
  import { prefs } from './prefs.svelte';
  import FileChanges from './FileChanges.svelte';
  import Notice from './Notice.svelte';
  import { CHANGES, changesView, fileRuns, openChanges } from './fileRuns.svelte';
  import Crumbs from './Crumbs.svelte';

  let { problem, beside = false }: { problem: string; beside?: boolean } = $props();
  const p = $derived(problems().find((x) => x.key === problem) ?? null);
  const f = $derived(p?.finding ?? null);
  const LEVEL = { critical: 'Critical', warning: 'Warning', info: 'To know' } as const;
  const WHERE: Record<string, string> = { library: 'Open the Library', playlists: 'Open Playlists', settings: 'Open Settings', health: 'Open Health', requests: 'Open Requests' };
  /** An Incoming album waiting (or not converted): compared with Music, its menu at hand (lib/HealthIncoming). */
  const incoming = $derived(problem.startsWith('incoming:'));
  /** The log events behind an item grouped from them (health.rs `grouped`): their area, event and grouping field. */
  const EVENTS: Record<string, { area: string; what: string; by: string }> = {
    'server:slow': { area: 'server', what: 'slow response', by: 'path' },
    'server:runaway': { area: 'server', what: 'client requests too often', by: 'client' },
    'sign-in:refused': { area: 'signin', what: 'request refused', by: 'path' },
    'sign-in:throttled': { area: 'signin', what: 'sign-in throttled', by: 'client' },
  };
  /** The findings Renumber fixes (it writes disc and track numbers): track numbers, and disc numbers (0.13, a5's
   *  `library:discs`). */
  const RENUMBERS = new Set(['library:track-numbers', 'library:discs']);

  /** An item's own key, the row open in place by it (the report read again keeps it open where it still is). */
  const keyOf = (x: HealthItem) => (x.track ? `t/${x.track}` : x.album ? `a/${x.album.join('/')}` : x.playlist ? `p/${x.playlist}` : `n/${x.title}`);
  let openKey = $state<string | null>(null);
  // Another problem: none open.
  $effect(() => {
    void problem;
    openKey = null;
  });
  const toggle = (x: HealthItem) => (openKey = openKey === keyOf(x) ? null : keyOf(x));
  /** A tool done with its item (renumbered, fixed, kept or replaced): closed, the report read again. */
  const gone = () => ((openKey = null), loadHealth());

  /** The album whose Change cover is open (Health's albums without a cover). */
  let coverOf = $state<{ id: string; title: string } | null>(null);
  /** The track whose better copy is being found (a file cut off or breaking off). */
  let betterOf = $state<{ source: string; album: string; track: string } | null>(null);
  /** A missing track being looked for (Albums missing tracks): its album and recording (lib/ReplaceTrackSheet). */
  let searchOf = $state<{ album: string; title: string; recording: string } | null>(null);
  const onward = (x: HealthItem) =>
    x.album ? { label: 'Open the album', go: () => viewOver(albumSubject(x.album![0], x.album![1])) } : x.playlist ? { label: 'Open the playlist', go: () => viewOver(x.playlist!) } : null;
  const since = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  const clock = (ms: number | null) => (ms ? `${Math.floor(ms / 60000)}:${String(Math.round((ms / 1000) % 60)).padStart(2, '0')}` : '');

  /** Tracks are looked for here: an administrator, where the server can. */
  const finds = $derived(me.admin && findsTracks());
  // The open album's missing tracks as its release has them (each its recording, to look for), read as it opens;
  // until then the report's names for them (the one loading rule: what is there shows, its Search waits for the release).
  let readAgain = $state(0);
  let release = $state<{ key: string; tracks: ReleaseTrack[] } | null>(null);
  const openItem = $derived(p?.items.find((x) => keyOf(x) === openKey) ?? null);
  $effect(() => {
    void readAgain;
    const x = openItem;
    if (!x?.missing?.length || x.album?.[0] !== 'cratarr' || !finds) return;
    const key = keyOf(x);
    let current = true;
    releaseOf(x.album[0], x.album[1]).then(
      (r) => current && (release = { key, tracks: r.tracks.filter((t) => !t.have).sort((a, b) => a.disc - b.disc || a.position - b.position) }),
      () => {},
    );
    return () => (current = false);
  });
  const missingOf = (x: HealthItem) => (release?.key === keyOf(x) ? release.tracks : null);

  // Work through (mockup C): the problem's tracks looked for one after another, the most missing albums first.
  const kind = $derived(p && finds ? throughKind(p.items) : null);
  /** The Work through under way, as it was planned when it started (a report read again meanwhile changes nothing). */
  let working = $state<{ stops: Stop[]; total: number } | null>(null);
  function workThrough() {
    if (!p || !kind) return;
    const stops = kind === 'missing' ? missingStops(p.items) : damagedStops(p.items);
    if (stops.length) working = { stops, total: throughTotal(stops) };
  }

  // Many at once (mockup D): albums ticked (as Downloads' Select, its bar's All and None), their missing tracks looked
  // for by Cratarr one search at a time; the review list over the albums while there is one.
  const bulk = $derived(kind === 'missing');
  const albumsOf = $derived(p && bulk ? p.items.filter((x) => x.album?.[0] === 'cratarr' && !!x.missing?.length) : []);
  let selecting = $state<Set<string> | null>(null);
  // Another problem: Select ends.
  $effect(() => {
    void problem;
    selecting = null;
  });
  const pickedAlbums = $derived(selecting ? albumsOf.filter((x) => selecting!.has(x.album![1])) : []);
  const pickedTracks = $derived(pickedAlbums.reduce((n, x) => n + (x.missing?.length ?? 0), 0));
  function tickAlbum(id: string) {
    if (!selecting) return;
    const s = new Set(selecting);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selecting = s;
  }
  let starting = $state(false);
  async function find() {
    if (!selecting?.size || starting) return;
    starting = true;
    try {
      intake = await startIntake([...selecting]);
      selecting = null;
    } catch (e) {
      toast.fail(e, "They weren't looked for");
    } finally {
      starting = false;
    }
  }

  /** The review list as Cratarr has it; ticks start off (nothing goes in without one), a copy picked per track. */
  let intake = $state<Intake | null>(null);
  let ticks = $state<Set<string>>(new Set());
  let picks = $state<Record<string, string>>({});
  let undone = $state<Set<string>>(new Set());
  async function readIntake() {
    try {
      const r = await getIntake();
      intake = r;
      ticks = keptTicks(untrack(() => ticks), r.items);
    } catch {
      /* none yet, or a Cratarr that cannot: no list */
    }
  }
  // Read once where it applies, then every couple of seconds while it is under way; the panel closing stops it.
  $effect(() => {
    if (bulk) untrack(() => void readIntake());
  });
  const polling = $derived(bulk && underWay(intake));
  $effect(() => {
    if (!polling) return;
    const t = setInterval(() => void readIntake(), 2000);
    return () => clearInterval(t);
  });
  // Done (looked for, the approved ones added): the report read again, its albums as they are now.
  let wasPolling = false;
  $effect(() => {
    const now = polling;
    if (wasPolling && !now) loadHealth();
    wasPolling = now;
  });
  const found = $derived(intake ? foundIds(intake.items) : []);
  const allTicked = $derived(found.length > 0 && found.every((id) => ticks.has(id)));
  const payload = $derived(intake ? approvePayload(intake.items, ticks, picks) : []);
  function tickItem(id: string) {
    const s = new Set(ticks);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    ticks = s;
  }
  let approving = $state(false);
  async function addTicked() {
    if (!payload.length || approving) return;
    approving = true;
    try {
      await approveIntake(payload);
      ticks = new Set();
      await readIntake();
    } catch (e) {
      toast.fail(e, "They weren't added");
    } finally {
      approving = false;
    }
  }
  async function clearFinished() {
    try {
      await clearIntake();
      await readIntake();
    } catch (e) {
      toast.fail(e, "The list wasn't cleared");
    }
  }
  async function undoItem(i: IntakeItem) {
    if (!i.undo) return;
    try {
      await undo([i.undo]);
      undone = new Set([...undone, i.id]);
      loadHealth();
    } catch (e) {
      toast.fail(e, "It wasn't undone");
    }
  }
  /** A track of the review list tapped: its copies to pick from, looking for it by hand, or out of the list. */
  let pickOf = $state<IntakeItem | null>(null);
  async function dropItem(i: IntakeItem) {
    pickOf = null;
    try {
      await dropIntake(i.id);
      await readIntake();
    } catch (e) {
      toast.fail(e, "It wasn't removed");
    }
  }

  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  const pill = () => (!beside ? { device: false, filter: false, view: false, search: 'health' } : null);
  handGlass(() => handed, { left: glassLeft, pill });
  // An album missing tracks: the whole release asked for (Lidarr fetches it into Incoming; Replace carries over).
  let asking = $state(false);
  let askedFull = $state<string | null>(null);
  async function askFull(x: HealthItem) {
    if (!x.release_group || asking) return;
    asking = true;
    try {
      await wantAlbum({ release_group: x.release_group, artist: (x.line ?? '').split(' · ')[0], album: x.title });
      askedFull = x.release_group;
    } catch (e) {
      toast.fail(e, "The album wasn't asked for");
    } finally {
      asking = false;
    }
  }
  // A library finding's fixes (0.9.7, the administrators' metadata workflow): read for its list (Apply all certain) and
  // its items' rows (lib/HealthFixes).
  const fixable = $derived(me.admin && FIXABLE.has(problem));
  $effect(() => {
    if (fixable) readFixes(problem);
  });
  const certain = $derived(fixable ? (healthFixes[problem]?.certain ?? 0) : 0);
  let applyingAll = $state(false);
  let askAll = $state(false);
  async function applyAll() {
    askAll = false;
    applyingAll = true;
    await apply({ certain: problem }, problem, `Applied ${certain} certain ${certain === 1 ? 'fix' : 'fixes'}`);
    applyingAll = false;
  }
  // Every problem and Changes to files, in Health's areas, each area's most urgent first (as the page lists them).
  const parts = $derived([
    ...byArea(problems(), AREAS).flatMap((g) => g.problems.map((x) => ({ id: x.key, group: g.area as string, label: x.finding?.thing ?? x.name }))),
    ...(me.admin ? [{ id: CHANGES, group: 'Activity', label: 'Changes to files' }] : []),
  ]);
  const shownId = $derived(problem === CHANGES ? CHANGES : (p?.key ?? null));
  const areaNow = $derived(parts.find((x) => x.id === shownId)?.group ?? null);
  // The levels over a problem: Health (not beside its list, which names it), then its area as a picker of the areas
  // (lib/Crumbs, lib/groupCrumb; the owner, 2026-11-02: the problems' one picker was "way too tall"), landing on the
  // problem last seen in one, else its most urgent. Not the problem's domain: no level of its own (it led to Health
  // too), and often the problem's name again (the owner, 2026-10-26: "Health › Analysis › Analysis").
  const areaLevel = $derived<Level | null>(
    areaNow
      ? {
          label: areaNow,
          pick: {
            menuLabel: 'Health areas',
            options: groupsOf(parts).map((a) => ({ id: a, label: a })),
            chosen: areaNow,
            onchoose: (a) => {
              const to = a === areaNow ? null : landingOf(parts, a, prefs.groupLast[`health/${a}`]);
              if (to) viewSibling(HEALTH + to);
            },
          },
        }
      : null,
  );
  const top = $derived<Level[]>(beside ? [] : [{ label: 'Health', go: () => upToPage('#/health') }, ...(areaLevel ? [areaLevel] : [])]);
  // The problem seen, remembered for its area's next pick.
  $effect(() => {
    if (areaNow && shownId) prefs.groupLast = remembered(prefs.groupLast, 'health', areaNow, shownId);
  });
  // Changes to files (decision 0092): its list, a plan to review, a run, under Health as a problem is.
  const changes = $derived(problem === CHANGES ? changesView(healthThing(panel.viewed)) : null);
  // The levels Back walks through (lib/Crumbs, one rule for every panel; the owner, 2026-11-01): Changes to files above a
  // run only where the run was opened from it (a step), and a tap there is that step back, never a jump to the list.
  const changesTrail = $derived(changes && changes.view !== 'list' && isStep() ? [...top, { label: 'Changes to files', go: () => history.back() }] : top);
  const changesHere = $derived(
    !changes ? '' : changes.view === 'list' ? 'Changes to files' : changes.view === 'plan' ? planTitle(changes.kind) : (fileRuns.list?.find((r) => r.id === changes.id)?.title ?? 'A change to files'),
  );
  // On a phone the problem's name is a picker of the other problems of its area, Changes to files among Activity's (a
  // move sideways, not up to Health's list and down again).
  const siblings = $derived.by<Siblings | null>(() => {
    const own = parts.filter((x) => x.group === areaNow);
    return beside || own.length < 2 ? null : { menuLabel: `${areaNow} shows`, options: own.map((x) => ({ id: x.id, label: x.label })), chosen: shownId, onchoose: (id) => viewSibling(HEALTH + id) };
  });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {#if !beside}
    <span class="navpill"><button aria-label="Back" title="Back" onclick={backOne}><Icon name="back" size={22} /></button></span>
  {/if}
  {#if changes}
    <Crumbs trail={changesTrail} here={changesHere} siblings={changes.view === 'list' ? siblings : null} />
  {:else if p}
    <Crumbs trail={top} here={p.name} {siblings} />
  {:else}
    <h2 class="title">Health</h2>
  {/if}
{/snippet}

<!-- What can be done with an item, open under its row. -->
{#snippet opened(x: HealthItem)}
  {@const on = onward(x)}
  <div class="opened">
    {#if x.playlist}<div class="tool"><HealthMissing playlist={x.playlist} /></div>{/if}
    {#if incoming && x.album}<div class="tool"><HealthIncoming source={x.album[0]} album={x.album[1]} ongone={gone} /></div>{/if}
    <!-- Track numbers off (unnumbered ones too), disc numbers: renumbered as a release has them (lib/HealthRenumber). -->
    {#if RENUMBERS.has(problem) && x.album?.[0] === 'cratarr' && offers('renumber')}<div class="tool"><HealthRenumber album={x.album[1]} title={x.title} ongone={gone} /></div>{/if}
    <!-- Year, genre, album artist, MusicBrainz release, a rule's names: the fix proposed, Apply (lib/HealthFixes). -->
    {#if fixable && x.album?.[0] === 'cratarr'}<div class="tool"><HealthFixes finding={problem} album={x.album[1]} track={x.track ?? null} title={x.title} ongone={gone} /></div>{/if}
    {#if EVENTS[problem]}<div class="tool"><HealthEvents {...EVENTS[problem]} value={x.title} /></div>{/if}
    {#if x.missing?.length}
      <!-- An album missing tracks (0.9.7; the owner, 2026-10-26: "golden work"): each one, looked for on Soulseek one at a
           time (decision 0080, lib/ReplaceTrackSheet); the whole album asked for. -->
      {@const tracks = missingOf(x)}
      {#if tracks}
        {#each tracks as t (t.recording)}
          <Item name={`${t.disc > 1 ? `${t.disc}-` : ''}${t.position}. ${t.title}`} line={clock(t.lengthMs) || null}>
            {#if finds && x.album}<button onclick={() => (searchOf = { album: x.album![1], title: x.title, recording: t.recording })}>Search</button>{/if}
          </Item>
        {/each}
      {:else}
        {#each x.missing as m (m)}
          <Item name={m}>{#if finds && x.album?.[0] === 'cratarr'}<button disabled aria-busy="true">Search</button>{/if}</Item>
        {/each}
      {/if}
      {#if x.release_group}
        {#if x.request || askedFull === x.release_group}
          <Item name="Asked for the full album" />
        {:else}
          <Item name="Ask for the full album" busy={asking} disabled={asking} onclick={() => askFull(x)} />
        {/if}
      {/if}
    {/if}
    <!-- A folder's files not heard yet, by name. -->
    {#each x.files ?? [] as file (file)}<Item name={file} wrap />{/each}
    <!-- A track's file cut off or breaking off (2026-10-28): a better copy of it, as the track's menu finds one. -->
    {#if x.track && x.album && finds}<Item name="Find a better copy" onclick={() => (betterOf = { source: x.album![0], album: x.album![1], track: x.track! })} />{/if}
    <!-- No cover: Change cover, straight on its choices (Upload and an address in its corner). -->
    {#if problem === 'library:cover' && x.album?.[0] === 'cratarr' && me.admin}<Item name="Change cover" onclick={() => (coverOf = { id: x.album![1], title: x.title })} />{/if}
    {#if on}<Item name={on.label} onclick={on.go} />{/if}
  </div>
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" class:beside>
      {@render glassLeft()}
      {#if pill()}<ContentControls {...pill()!} />{/if}
    </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()}>
    {#if changes}
      <FileChanges thing={healthThing(panel.viewed)} />
    {:else if !p}
      <!-- Not found after a failed read is not "gone" (the owner, 2026-11-02: errors are never shown as nothing). -->
      {#if health.reportError || health.error}<Notice text={health.reportError ?? health.error ?? ''} onretry={loadHealth} />{:else if health.loaded}<p class="muted">This is no longer a problem.</p>{/if}
    {:else}
      <!-- How it stands, and what it means behind its (i). -->
      <p class="state" class:urgent={p.urgent}>
        <Icon name={p.icon} size={20} />
        <span>{f ? LEVEL[f.level] : p.urgent ? 'Critical' : 'Warning'}{#if f?.since}<span class="muted"> · since {since(f.since)}</span>{/if}</span>
        {#if f?.detail || p.hint}<MoreInfo label={p.name} text={(f?.detail ?? p.hint)!} />{/if}
      </p>
      <!-- How much. -->
      {#if f && f.of}
        <div class="meter" role="meter" aria-valuemin="0" aria-valuemax={f.of} aria-valuenow={f.count} aria-label={p.title}>
          <!-- What it is, in its sentence (the owner, 2026-10-25), over how much of the whole. -->
          <div class="head"><span>{p.said}</span><b>{Math.round((100 * f.count) / f.of)}%</b></div>
          <div class="track"><i style:width={`${(100 * f.count) / f.of}%`}></i></div>
        </div>
      {:else}
        <p class="count"><b>{p.said}</b></p>
      {/if}
      <!-- All of it at once where it can be: its tracks in turn (Work through), every certain fix (asked once more). -->
      {#if kind || certain > 0}
        <Group>
          {#if kind}
            <Item name={kind === 'missing' ? 'Work through the missing tracks' : 'Work through the damaged files'} line={kind === 'missing' ? 'One at a time, most missing albums first' : 'One at a time, album by album'}>
              <button class="primary" onclick={workThrough}>Start</button>
            </Item>
          {/if}
          {#if bulk && albumsOf.length}
            {#if selecting}
              <Item name="Find missing tracks" line={`${plural(pickedAlbums.length, 'album')} · ${plural(pickedTracks, 'track')} · each approved by you`} />
            {:else}
              <Item name="Find missing tracks" line="Many at once, each approved by you" onclick={() => ((openKey = null), (selecting = new Set()))} />
            {/if}
          {/if}
          {#if certain > 0}
            <Item name={`Apply all certain (${certain.toLocaleString()})`} busy={applyingAll} disabled={applyingAll} onclick={() => (askAll = true)} />
          {/if}
        </Group>
      {/if}
      <!-- The review list (mockup D): each track looked for, its chosen copy or where it stands; ticked ones added. -->
      {#if bulk && intake?.items.length}
        {@const look = lookedFor(intake.items)}
        <Group title={intake.running ? `Looking for ${plural(look.total, 'missing track')}` : `Found ${found.length} of ${look.total}`}>
          {#snippet tail()}{#if intake && anyOver(intake.items)}<button class="tailact" onclick={() => void clearFinished()}>Clear finished</button>{/if}{/snippet}
          {#if intake.running}<Item name="Looking for them" line={`${look.done} of ${look.total}`} progress={look.total ? look.done / look.total : 0} />{/if}
          {#if found.length}
            <Item name={`${ticks.size} of ${found.length} ticked`}>
              <button onclick={() => (ticks = allTicked ? new Set() : new Set(found))}>{allTicked ? 'None' : 'Tick all found'}</button>
              <button class="primary" disabled={!payload.length || approving} aria-busy={approving || undefined} onclick={() => void addTicked()}>{payload.length ? `Add ${payload.length}` : 'Add'}</button>
            </Item>
          {/if}
          {#each intake.items as i (i.id)}
            <ReleaseRow
              cover={albumCoverUrl(i.album.source, i.album.id)}
              title={`${i.title} · ${i.album.title}`}
              line={undone.has(i.id) ? 'Undone' : itemLine(i, picks[i.id])}
              tone={undone.has(i.id) ? null : itemTone(i)}
              wrap={!!itemEnd(i)}
              onclick={() => (pickOf = i)}
            >
              {#if tickable(i)}
                {@const on = ticks.has(i.id)}
                <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Tick ${i.title}`} onclick={() => tickItem(i.id)}
                  ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
                >
              {:else if i.state === 'added' && i.undo && !undone.has(i.id)}
                <button onclick={() => void undoItem(i)}>Undo</button>
              {:else if itemEnd(i)}
                <span class="ended">{itemEnd(i)}</span>
              {/if}
            </ReleaseRow>
          {/each}
        </Group>
      {/if}
      <!-- The things it is about, one by one: an album a release row (lib/ReleaseRow), anything else a settings row; a tap
           opens it in place. -->
      {#if p.items.length}
        <Group>
          {#each p.items as x, i (i)}
            {#if !matchesSearch(x.title, x.line)}<!-- left out by the search -->
            {:else}
              {@const open = openKey === keyOf(x)}
              {#if x.album && selecting && x.album[0] === 'cratarr' && x.missing?.length}
                {@const id = x.album[1]}
                {@const on = selecting.has(id)}
                <ReleaseRow cover={albumCoverUrl(x.album[0], id)} title={x.title} line={x.line ?? null} onclick={() => tickAlbum(id)}>
                  <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Select ${x.title}`} onclick={() => tickAlbum(id)}
                    ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
                  >
                </ReleaseRow>
              {:else if x.album}
                <ReleaseRow cover={albumCoverUrl(x.album[0], x.album[1])} title={x.title} line={x.line ?? null} expanded={open} onclick={() => toggle(x)} />
              {:else}
                <Item name={x.title} line={x.line ?? null} expanded={open} onclick={() => toggle(x)} />
              {/if}
              {#if open}{@render opened(x)}{/if}
            {/if}
          {/each}
        </Group>
      {/if}
      <!-- The way to deal with it. -->
      {#if f?.open}
        <Group><Item name={WHERE[f.open.page] ?? 'Open'} onclick={() => f && openFinding(f)} /></Group>
      {/if}
    {/if}
    <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

{#if coverOf}
  <AlbumMenu source="cratarr" album={{ id: coverOf.id, title: coverOf.title, artist: null }} start="cover" onclose={() => ((coverOf = null), loadHealth())} />
{/if}
{#if betterOf}
  <ReplaceTrackSheet source={betterOf.source} album={{ id: betterOf.album, title: '' }} replace={{ source: betterOf.source, id: betterOf.track }} onclose={() => ((betterOf = null), loadHealth())} />
{/if}
{#if searchOf}
  <ReplaceTrackSheet source="cratarr" album={{ id: searchOf.album, title: searchOf.title }} start={searchOf.recording} onclose={() => ((searchOf = null), readAgain++, loadHealth())} />
{/if}
{#if working}
  <ReplaceTrackSheet
    source={working.stops[0].source}
    album={{ id: working.stops[0].album, title: working.stops[0].title }}
    through={working}
    onclose={() => ((working = null), readAgain++, loadHealth())}
  />
{/if}
{#if selecting}
  <SelectBar
    label="Selected albums"
    count="{pickedAlbums.length}/{albumsOf.length}"
    onclose={() => (selecting = null)}
    every={{
      all: albumsOf.length > 0 && pickedAlbums.length === albumsOf.length,
      onall: () => (selecting = new Set(albumsOf.map((x) => x.album![1]))),
      onnone: () => (selecting = new Set()),
    }}
  >
    <button aria-label="Find missing tracks" title="Find missing tracks" disabled={!pickedAlbums.length || starting} aria-busy={starting || undefined} onclick={() => void find()}
      ><Icon name="search" size={24} /></button
    >
  </SelectBar>
{/if}
{#if pickOf}
  {@const i = pickOf}
  <MenuSheet label="A missing track" icon="search" title={i.title} chips={[i.album.title, itemLine(i, picks[i.id])]} onclose={() => (pickOf = null)}>
    <MenuCards>
      {#if tickable(i)}
        <!-- Its copies, the best first: the one picked goes in once it is ticked and added. -->
        <MenuGroup>
          {#each i.candidates as c (c.id)}
            <MenuEntry
              icon={copyOf(i, picks[i.id])?.id === c.id ? 'check' : 'arrowDownToLine'}
              label={c.file.split(/[\\/]/).pop() ?? c.file}
              sub={copyLine(c)}
              onclick={() => ((picks = { ...picks, [i.id]: c.id }), (pickOf = null))}
            />
          {/each}
        </MenuGroup>
      {/if}
      <MenuGroup>
        {#if finds && i.state !== 'added'}<MenuEntry icon="search" label="Look for it yourself" onclick={() => ((searchOf = { album: i.album.id, title: i.album.title, recording: i.recording }), (pickOf = null))} />{/if}
        <MenuEntry icon="close" label="Remove from the list" danger onclick={() => void dropItem(i)} />
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
{#if askAll}
  <ConfirmSheet
    title={`Apply ${certain.toLocaleString()} certain ${certain === 1 ? 'fix' : 'fixes'}?`}
    text="Written into the files; Undo puts the old values back."
    label="Apply"
    icon="check"
    onconfirm={applyAll}
    onclose={() => (askAll = false)}
  />
{/if}

<style>
  /* As a part of the settings in the panel (lib/SettingsPanel): its bar on top, what it holds under it. */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 65px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar.beside {
    padding-left: clamp(12px, 2vw, 24px);
  }

  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .muted {
    color: var(--muted);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 16px;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  p {
    margin: 0;
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  .state {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 500;
  }

  .state :global(svg) {
    color: var(--accent);
  }

  .state.urgent :global(svg) {
    color: var(--danger);
  }

  .meter .head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 6px;
    font-variant-numeric: tabular-nums;
  }

  .track {
    height: 8px;
    overflow: hidden;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .track i {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
  }

  /* An item open in place: what can be done with it, under its row, set in from its cover (Settings' rows, its own
     lines between them). */
  .opened {
    padding-left: 58px;
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }

  .opened > :global(* + *) {
    border-top: 1px solid var(--border);
  }

  .tool {
    display: grid;
    gap: 12px;
    padding: 12px 16px 12px 0;
  }

  /* A row's tick (as Downloads' and Requests' Select). */
  /* Where it went wrong, at the row's end; why under its title. */
  .ended {
    flex: none;
    padding: 0 8px;
    color: var(--danger-ink);
    font-size: 13px;
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

  /* Clear finished, at the review list's heading's end. */
  .tailact {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    cursor: pointer;
  }

  .count b {
    font-size: 1.6rem;
    font-weight: 650;
  }

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }
</style>
