<script lang="ts">
  // The visuals on the rack (decision 0057, the switch): what shows is the patch in the store (visuals-patch.svelte.ts),
  // drawn by the loop (visuals-loop.ts: the one engine, the compositor), changed by the panel
  // (VisualsPanel.svelte) and the graph (PatchGraph) alike, every change one undo step. Over everything,
  // on black: a tap shows the track, the controls and the dice for a few seconds; two or more on a side seek; a swipe
  // down, Esc or Back closes. The dice: a tap opens a row of its scopes (Everything, Sources…), each a roll of what is not pinned; the row stays, and
  // holds the controls up, till the dice is tapped again. A long press rolls everything. Reset:
  // the track's own look again, or everything (ResetMenu). Snapshots: saved patches on the server, every device's.
  import { onMount, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { getVisualsSnapshots, type VisualsSnapshot } from '../api';
  import Icon from '../Icon.svelte';
  import { longpress } from '../longpress';
  import { sheetDrag } from '../sheetDrag';
  import { reducedMotion } from '../motion';
  import { player } from '../live.svelte';
  import { panel as sidePanel } from '../panel.svelte';
  import { prefs } from '../prefs.svelte';
  import { paused, playing, seekBy, send } from '../remote.svelte';
  import { stopMusic } from './music.svelte';
  import { setVisualsSide, visuals } from './shown.svelte';
  import { STANDARD } from './shared.svelte';
  import { held } from './held.svelte';
  import Help from './Help.svelte';
  import SnapshotsSheet from './SnapshotsSheet.svelte';
  import VisualsPanel from './VisualsPanel.svelte';
  import ResetMenu from './ResetMenu.svelte';
  import PatchGraph from './PatchGraph.svelte';
  import { LiveFaces } from './faces.svelte';
  import { helpFor } from './module-help';
  import { applyEdit, type RackEdit } from './patch-edits';
  import { cratarrTypes } from './patch';
  import { v2Types } from './picture-graph';
  import { exampleOf } from './examples';
  import { libraryOf } from './library';
  import { RANDOMIZE_SCOPES } from './ports';
  import type { Category } from './rack';
  import { DEFAULTS, RANGES, visualSettings } from './settings.svelte';
  import { patchOfSnapshot, snapshotOfPatch } from './snapshot-v2';
  import { v2TypeRecord } from './v2-types';
  import { VisualsLoop } from './visuals-loop';
  import { emptyRack, visualsPatch } from './visuals-patch.svelte';

  // `docked`: beside the page, in the panel at the right (App.svelte), rather than over all of it.
  let { onclose, docked = false }: { onclose: () => void; docked?: boolean } = $props();

  let canvas: HTMLCanvasElement;
  let chrome = $state(true);
  let unsupported = $state(false);
  // Why some of the patch is not drawn (the renderer's errors), shown while the chrome is.
  let drawErrors = $state<string[]>([]);
  let panel = $state(false);
  let fps = $state(0);
  const np = $derived(playing());

  const TYPES = v2TypeRecord();
  const PICTURE_TYPES = v2Types(cratarrTypes(RANGES, DEFAULTS));
  let loop: VisualsLoop | null = null;

  // The controls show for a few seconds after a tap or a move; with the panel open, until it is closed.
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  function showChrome() {
    chrome = true;
    clearTimeout(hideTimer);
    if (!panel && !graph && !diceRow) hideTimer = setTimeout(() => (chrome = false), 3000);
  }

  // Closing: by the button, Esc or a swipe down this step of the history goes back too; Back has done that already.
  let closed = false;
  function close() {
    if (closed) return;
    closed = true;
    if (history.state?.visuals) history.back();
    onclose();
  }

  // A swipe down closes, a tap shows the controls.
  let down: { x: number; y: number } | null = null;
  function pointerDown(e: PointerEvent) {
    down = { x: e.clientX, y: e.clientY };
  }
  function pointerUp(e: PointerEvent) {
    if (!down) return;
    const dx = e.clientX - down.x;
    const dy = e.clientY - down.y;
    down = null;
    if (dy > 80 && Math.abs(dy) > 2 * Math.abs(dx)) close();
    else if (Math.hypot(dx, dy) < 10) tapped(e);
  }

  // Taps, as video players take them: one shows or hides the controls, once it is clear no second follows; two or
  // more in a row on one side seek, on the right forward and on the left back: two 10 s, three 30 s, each after 30 s more.
  const TAP_MS = 300;
  let taps: { n: number; side: 1 | -1; at: number; timer: ReturnType<typeof setTimeout> } | null = null;
  let sought = $state<{ side: 1 | -1; secs: number } | null>(null);
  let soughtTimer: ReturnType<typeof setTimeout> | undefined;
  function tapped(e: PointerEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const side = e.clientX - box.left > box.width / 2 ? 1 : -1;
    const now = performance.now();
    if (taps) clearTimeout(taps.timer);
    if (!(taps && taps.side === side && now - taps.at < TAP_MS)) {
      taps = { n: 1, side, at: now, timer: setTimeout(toggleChrome, TAP_MS) };
      return;
    }
    taps.n++;
    taps.at = now;
    taps.timer = setTimeout(() => (taps = null), TAP_MS);
    const step = taps.n === 2 ? 10 : taps.n === 3 ? 20 : 30;
    void seekBy(side * step);
    sought = { side, secs: (taps.n === 2 ? 0 : (sought?.secs ?? 0)) + step };
    clearTimeout(soughtTimer);
    soughtTimer = setTimeout(() => (sought = null), 900);
  }
  function toggleChrome() {
    taps = null;
    // The dice's row open holds the controls up.
    if (diceRow) return showChrome();
    if (chrome) {
      chrome = false;
      closeAll();
    } else showChrome();
  }
  // Panel and Rack (step 8, the owner): two toggles, both always in the corner, the panel and the graph each opened on
  // its own (the panel alone over the picture, the graph alone, or both). What is open is one step of the history: the
  // close button, Esc and Back close the last opened first, the visuals after (the owner, 71.1).
  let graph = $state(false);
  let opened: ('panel' | 'graph')[] = [];
  function opening(what: 'panel' | 'graph') {
    if (!opened.length && !history.state?.rack) history.pushState({ ...history.state, visuals: true, rack: true }, '');
    opened = [...opened.filter((x) => x !== what), what];
  }
  function shut(what: 'panel' | 'graph') {
    opened = opened.filter((x) => x !== what);
    if (what === 'panel') {
      panel = false;
      sheetDy = 0;
    } else graph = false;
  }
  function closing(what: 'panel' | 'graph') {
    shut(what);
    if (!opened.length && history.state?.rack) history.back();
    showChrome();
  }
  function closeAll() {
    const was = opened.length;
    for (const x of [...opened]) shut(x);
    if (was && history.state?.rack) history.back();
  }
  function openPanel(top = false) {
    if (top) panelPath = [];
    if (!panel) {
      panel = true;
      sheetDy = 0;
      void loadLibrary();
      opening('panel');
    }
    showChrome();
  }
  // The level the panel's page is at: the graph opens there.
  let panelLevel: string[] = [];
  function openGraph() {
    if (panel) graphLevel = panelLevel;
    graph = true;
    void loadLibrary();
    opening('graph');
    showChrome();
  }
  const togglePanel = () => (panel ? closing('panel') : openPanel(true));
  const toggleGraph = () => (graph ? closing('graph') : openGraph());
  /** The close button and Esc: the last opened first, then the visuals. */
  const closeOne = () => (opened.length ? closing(opened[opened.length - 1]) : close());

  // ---- The patch's changes ----------------------------------------------------------------------------------------
  // The preset the patch was last set to (its name in the panel), until something is changed.
  let preset = $state<string | null>(null);
  let presetPatch = $state.raw(visualsPatch.patch);
  const presetNow = $derived(visualsPatch.patch === presetPatch ? preset : null);
  function panelEdit(e: RackEdit, key: string | null) {
    visualsPatch.change((p) => applyEdit(p, e, TYPES), key);
    showChrome();
  }
  // The presets are the snapshot library's (the server's, every device's: the owner, "that snapshot library needs to
  // be used"), read when the rack opens; one chosen is recalled as the Snapshots sheet recalls one.
  let library = $state<VisualsSnapshot[]>([]);
  const loadLibrary = () => getVisualsSnapshots().then((l) => (library = l ?? []), () => {});
  // The one library (library.ts): the saved ones, then the example racks, as the Snapshots sheet lists them.
  const libraryList = $derived(libraryOf(library).map(({ id, label, line, group }) => ({ id, label, line, group })));
  function choosePreset(id: string) {
    const example = exampleOf(id);
    const snap = example ? null : library.find((x) => x.id === id);
    if (!example && !snap) return;
    visualsPatch.replace(example ? structuredClone(example.patch) : patchOfSnapshot(snap!.settings));
    preset = id;
    presetPatch = visualsPatch.patch;
    showChrome();
  }
  /** Everything back: the library's Default, else an empty rack. */
  function resetEverything() {
    const def = library.find((x) => x.name.replace(/ · \d\d:\d\d$/, '') === 'Default');
    if (def) return choosePreset(def.id);
    visualsPatch.replace(emptyRack());
    preset = null;
    showChrome();
  }
  // Opened by themselves (lib/visualsAuto; the owner, 2026-10-30): from the snapshot this device says, once it is
  // known (a saved one comes with the library), or as last seen; and, where it says so, a new look (the dice's
  // macros) at each new track while they stay.
  let autoStarted = false;
  $effect(() => {
    if (!visuals.auto || autoStarted) return;
    const id = prefs.visualsAutoStart;
    if (id === 'last') return void (autoStarted = true);
    if (exampleOf(id) || library.some((x) => x.id === id)) {
      autoStarted = true;
      untrack(() => choosePreset(id));
    }
  });
  let trackSeen: string | null = null;
  $effect(() => {
    const id = np ? `${np.source}:${np.id}` : null;
    const was = trackSeen;
    trackSeen = id;
    if (visuals.auto && prefs.visualsAutoRun === 'track' && was !== null && id !== null && id !== was) untrack(() => roll('macros'));
  });
  type Scope = (typeof RANDOMIZE_SCOPES)[number];
  function roll(scope: Scope | { module: string[] } = 'macros') {
    loop?.roll(scope);
    showChrome();
  }
  // A module's own Randomize, by its path (engine/dice.ts: a device's its macros, a module's its knobs); a category's whole.
  function randomizeFrom(what: { module: string[] } | { category: Category }) {
    if ('module' in what) roll({ module: what.module });
    // In / Out (Music, the Screen, the Meter) is no scope of the dice: nothing of it to roll.
    else if (what.category !== 'io') roll(what.category);
  }
  // The dice's row of scopes: open till the dice is tapped again.
  let diceRow = $state(false);
  // A scope pinned: every roll keeps what it reaches (Everything's included).
  const scopePinned = (s: Scope) => visualsPatch.patch.pins.includes(`scope:${s}`);
  function pinScope(s: Scope) {
    const id = `scope:${s}`;
    visualsPatch.change((p) => applyEdit(p, { kind: 'pinOne', scope: [], pin: id, on: !p.pins.includes(id) }, TYPES), null);
    showChrome();
  }
  const DICE_LABEL: Record<(typeof RANDOMIZE_SCOPES)[number], string> = { everything: 'Everything', sources: 'Sources', effects: 'Effects', mixers: 'Mixers', modulators: 'Modulators', cables: 'Cables', response: 'Response', macros: 'Macros', colours: 'Colours', spatial: 'Spatial', lightness: 'Lightness' };
  let resetMenu = $state(false);

  // ---- Snapshots: saved patches on the server ---------------------------------------------------------------------
  let snapshotsOpen = $state(false);
  function recall(snap: VisualsSnapshot) {
    if (snap.id === STANDARD.id) return resetEverything();
    visualsPatch.replace(patchOfSnapshot(snap.settings));
    preset = null;
    showChrome();
  }

  // ---- The rack open: the graph is the page, the panel its inspector (0057 §7b) -----------------------------------
  // Where there is room the panel is a column beside the graph; on a narrow screen (a phone, the visuals beside the
  // page) a sheet over it from below, as tall as its view needs, closed by a drag down, a tap on the graph or Back, the
  // graph untouched underneath. Its handle brings it back at its top view.
  let wide = $state(typeof innerWidth === 'number' ? innerWidth >= 1100 : false);
  const sheet = $derived(!wide || docked);
  const graphShown = $derived(graph);
  let sheetDy = $state(0);
  const panelShown = $derived(panel);
  let graphLevel = $state<string[]>([]);
  let panelPath = $state<string[]>([]);
  const liveFaces = new LiveFaces();
  const GRAPH_KEY = 'cratarr-visuals-rack-graph';
  let graphAt = $state<Record<string, { x: number; y: number }>>(
    (() => {
      try {
        return JSON.parse(localStorage.getItem(GRAPH_KEY) ?? '{}');
      } catch {
        return {};
      }
    })(),
  );
  function graphMove(id: string, x: number, y: number) {
    graphAt = { ...graphAt, [id]: { x, y } };
    try {
      localStorage.setItem(GRAPH_KEY, JSON.stringify(graphAt));
    } catch {
      /* this visit only */
    }
  }
  // A module, cable or knob tapped on the graph: the panel opened on it.
  const graphPage = (page: string) => {
    panelPath = [page];
    openPanel();
  };

  // ---- Full screen, beside the page -------------------------------------------------------------------------------
  const canFullscreen = typeof document !== 'undefined' && !!document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function';
  let fullscreen = $state(typeof document !== 'undefined' && !!document.fullscreenElement);
  function goFullscreen() {
    document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
    if (docked) setVisualsSide(false);
    showChrome();
  }
  function leaveFullscreen() {
    document.exitFullscreen().catch(() => {});
    showChrome();
  }
  function beside() {
    prefs.sidebar = true;
    player.min = false;
    player.full = false;
    sidePanel.playerTop = false;
    setVisualsSide(true);
  }

  onMount(() => {
    showChrome();
    if (!history.state?.visuals) history.pushState({ ...history.state, visuals: true }, '');
    const popped = () => {
      // Back closes one thing at a time: the last opened of the panel and the graph, the other after, then the visuals.
      const st = history.state ?? {};
      if (opened.length && !st.rack) {
        shut(opened[opened.length - 1]);
        if (opened.length) history.pushState({ ...history.state, visuals: true, rack: true }, '');
      }
      if (!closed && !st.visuals) {
        closed = true;
        onclose();
      }
    };
    addEventListener('popstate', popped);
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && closeOne();
    addEventListener('keydown', onKey);
    const resized = () => (wide = innerWidth >= 1100);
    addEventListener('resize', resized);
    if (!docked) document.documentElement.requestFullscreen?.({ navigationUI: 'hide' }).catch(() => {});
    let lock: WakeLockSentinel | null = null;
    const wake = () => {
      if (document.visibilityState !== 'visible' || !('wakeLock' in navigator)) return;
      navigator.wakeLock.request('screen').then(
        (l) => (lock = l),
        () => {},
      );
    };
    wake();
    document.addEventListener('visibilitychange', wake);
    const fullscreenChanged = () => (fullscreen = !!document.fullscreenElement);
    document.addEventListener('fullscreenchange', fullscreenChanged);

    loop = new VisualsLoop({
      canvas,
      store: visualsPatch,
      device: () => visualSettings,
      types: TYPES,
      pictureTypes: PICTURE_TYPES,
      onPace: (p) => (fps = p.fps),
      onFrame: (frame) => {
        if (graphShown || panelShown) liveFaces.offer(performance.now(), () => ({ time: performance.now() / 1000, beat: null, outputs: { ...frame.outputs }, points: frame.points }));
      },
      onUnsupported: () => (unsupported = true),
      onErrors: (errors) => (drawErrors = errors),
    });
    loop.start();

    return () => {
      if (!closed && !visuals.open && history.state?.visuals) history.back();
      loop?.stop();
      loop = null;
      clearTimeout(hideTimer);
      removeEventListener('popstate', popped);
      removeEventListener('keydown', onKey);
      removeEventListener('resize', resized);
      document.removeEventListener('visibilitychange', wake);
      document.removeEventListener('fullscreenchange', fullscreenChanged);
      lock?.release().catch(() => {});
      if (!docked && document.fullscreenElement) document.exitFullscreen().catch(() => {});
      stopMusic();
    };
  });
</script>

<div
  class="visuals"
  class:docked
  class:bare={!chrome}
  role="dialog"
  aria-modal="true"
  aria-label="Visuals"
  tabindex="-1"
  onpointerdown={pointerDown}
  onpointerup={pointerUp}
  onpointermove={(e) => e.pointerType === 'mouse' && !down && showChrome()}
>
  <canvas bind:this={canvas}></canvas>
  {#if unsupported}
    <p class="note">This device cannot draw the visuals.</p>
  {/if}
  {#if chrome && drawErrors.length}
    <p class="refused" role="status">{drawErrors[0]}{drawErrors.length > 1 ? ` (+${drawErrors.length - 1})` : ''}</p>
  {/if}
  {#if chrome}
    <div class="chrome" transition:fade={{ duration: reducedMotion() ? 0 : 200 }}>
      <button class="close" aria-label={panel || graph ? 'Close the rack' : 'Close the visuals'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={closeOne}
        ><Icon name="close" size={26} /></button
      >
      <div class="corner">
        <!-- Rack then Panel, as the graph is at the left and the panel at the right (the owner). -->
        <button class="round" class:on={graph} aria-label="Rack" aria-expanded={graph} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={toggleGraph}
          ><Icon name="rack" size={24} /></button
        >
        <button class="round" class:on={panel} aria-label="Panel" aria-expanded={panel} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={togglePanel}
          ><Icon name="sliders" size={24} /></button
        >
        <button
          class="round"
          class:on={diceRow}
          aria-label="Randomize"
          aria-expanded={diceRow}
          use:held={'randomise'}
          use:longpress={() => (roll('everything'), showChrome())}
          onpointerdown={(e) => e.stopPropagation()}
          onpointerup={(e) => e.stopPropagation()}
          onclick={() => ((diceRow = !diceRow), showChrome())}><Icon name="dice" size={24} /></button
        >
        {#if visualsPatch.canUndo}
          <button
            class="round"
            aria-label="Undo"
            use:held={'undo'}
            onpointerdown={(e) => e.stopPropagation()}
            onpointerup={(e) => e.stopPropagation()}
            onclick={() => {
              visualsPatch.undo();
              showChrome();
            }}><Icon name="undo" size={24} /></button
          >
        {/if}
        <button
          class="round"
          aria-label="Snapshots"
          aria-haspopup="dialog"
          use:held={'snapshots'}
          onpointerdown={(e) => e.stopPropagation()}
          onpointerup={(e) => e.stopPropagation()}
          onclick={() => {
            snapshotsOpen = true;
            showChrome();
          }}><Icon name="bookmark" size={24} /></button
        >
        {#if docked}
          <button class="round" aria-label="Over the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => setVisualsSide(false)}
            ><Icon name="expand" size={22} /></button
          >
        {:else if player.roomy}
          <button class="round" aria-label="Beside the page" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={beside}
            ><Icon name="sidebar" size={24} /></button
          >
        {/if}
        {#if canFullscreen}
          <button
            class="round"
            aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}
            onpointerdown={(e) => e.stopPropagation()}
            onpointerup={(e) => e.stopPropagation()}
            onclick={fullscreen ? leaveFullscreen : goFullscreen}><Icon name={fullscreen ? 'exitFullscreen' : 'fullscreen'} size={24} /></button
          >
        {/if}
      </div>
      {#if diceRow}
        <div class="dice-row" role="toolbar" aria-label="Randomize" transition:fade={{ duration: 120 }}>
          {#each RANDOMIZE_SCOPES as scope (scope)}
            <div class="scope" class:held={scope !== 'everything' && scopePinned(scope)}>
              <button onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => (roll(scope), showChrome())}>{DICE_LABEL[scope]}</button>
              {#if scope !== 'everything'}
                <button
                  class="pin"
                  class:on={scopePinned(scope)}
                  aria-label={`Pin ${DICE_LABEL[scope]}`}
                  aria-pressed={scopePinned(scope)}
                  onpointerdown={(e) => e.stopPropagation()}
                  onpointerup={(e) => e.stopPropagation()}
                  onclick={() => pinScope(scope)}><Icon name="pin" size={16} /></button
                >
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      {#if graphShown}
        <div
          class="graph"
          class:whole={sheet || !panel}
          role="group"
          aria-label="Rack"
          onpointerdown={(e) => {
            e.stopPropagation();
            // A tap on the graph closes the sheet over it (a module tapped opens it again, on its view).
            if (sheet && panel) closing('panel');
          }}
          onpointerup={(e) => e.stopPropagation()}
        >
          <PatchGraph
            patch={visualsPatch.patch}
            onchange={(next, key) => {
              visualsPatch.change(() => next, key ?? null);
              showChrome();
            }}
            types={TYPES}
            bind:at={graphLevel}
            positions={graphAt}
            onmove={graphMove}
            onopen={(module) => graphPage(`m|${module.join('/')}`)}
            onopencable={(cable) => graphPage(`c|${cable.slice(0, -1).join('/')}|${cable[cable.length - 1]}`)}
            onopenknob={(module, knob) => graphPage(`in|${module.join('/')}|${knob}`)}
            onpick={(r) => graphPage(`from|${r.module.slice(0, -1).join('/')}|${r.module[r.module.length - 1]}|${r.knob}`)}
            live={liveFaces}
            thumbnail={(m, c) => (loop ? loop.thumbnail(m, c) : 'Not running.')}
            previewShown={sheet && panel}
            tapOpens={!panel}
            {helpFor}
          />
        </div>
      {/if}
      {#if panelShown}
        <div
          class="panel"
          class:sheet
          role="group"
          aria-label="Panel"
          style:transform={sheet && sheetDy ? `translateY(${sheetDy}px)` : undefined}
          use:sheetDrag={{ onclose: () => closing('panel'), ondrag: (d) => (sheetDy = d), enabled: sheet }}
          onpointerdown={(e) => e.stopPropagation()}
          onpointerup={(e) => e.stopPropagation()}
        >
          <button class="panel-close" aria-label="Close the panel" onclick={() => closing('panel')}><Icon name="close" size={20} /></button>
          <VisualsPanel
            patch={visualsPatch.patch}
            types={TYPES}
            onedit={panelEdit}
            presets={libraryList}
            preset={presetNow}
            onpreset={choosePreset}
            onrandomize={randomizeFrom}
            live={liveFaces.frame}
            bind:path={panelPath}
            thumbnail={(m, c) => (loop ? loop.thumbnail(m, c) : 'Not running.')}
            previewShown={!graph}
            onlevel={(scope) => {
              panelLevel = scope;
              if (graph && scope.join('/') !== graphLevel.join('/')) graphLevel = scope;
            }}
          />
          {#if !panelPath.length}
            <div class="foot">
              <span class="fps">{fps} fps</span>
              <button class="reset" use:held={'reset'} onclick={() => (resetMenu = true)}>Reset</button>
            </div>
          {/if}
        </div>
      {/if}
      {#if np && !panel && !graph}
        <div class="track">
          <strong>{np.name}</strong>
          {#if np.artist}<span>{np.artist}</span>{/if}
        </div>
        <div class="transport">
          <button class="round" aria-label="Previous track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('previous')}
            ><Icon name="previous" size={26} /></button
          >
          <button class="round" aria-label={paused() ? 'Play' : 'Pause'} onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('play-pause')}
            ><Icon name={paused() ? 'play' : 'pause'} size={26} /></button
          >
          <button class="round" aria-label="Next track" onpointerdown={(e) => e.stopPropagation()} onpointerup={(e) => e.stopPropagation()} onclick={() => send('next')}
            ><Icon name="next" size={26} /></button
          >
        </div>
      {/if}
    </div>
  {/if}
  {#if sought}
    <div class="sought" class:back={sought.side < 0} aria-live="polite">{sought.side > 0 ? '+' : '−'}{sought.secs} s</div>
  {/if}
  {#if resetMenu}
    <ResetMenu onseed={() => loop?.resetToSeed()} oneverything={resetEverything} onclose={() => (resetMenu = false)} />
  {/if}
  {#if snapshotsOpen}
    <SnapshotsSheet
      onclose={() => (snapshotsOpen = false)}
      capture={() => loop?.picture() ?? Promise.resolve(null)}
      trackName={np?.name ?? null}
      {recall}
      onexample={choosePreset}
      settingsNow={() => snapshotOfPatch(visualsPatch.patch)}
      over={!docked}
    />
  {/if}
  <Help />
</div>

<style>
  .visuals {
    position: fixed;
    inset: 0;
    z-index: 2000;
    background: #000;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Beside the page: in the panel at the right, under the page's own layers. */
  .visuals.docked {
    position: absolute;
    z-index: 1;
  }

  .visuals.bare {
    cursor: none;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }

  .refused {
    position: absolute;
    inset: max(12px, env(safe-area-inset-top)) 72px auto 72px;
    z-index: 2;
    margin: 0;
    color: #f3c46b;
    text-align: center;
    font-size: 0.85rem;
    pointer-events: none;
  }

  .note {
    position: absolute;
    inset: auto 0 45% 0;
    margin: 0;
    color: #bbb;
    text-align: center;
    font-size: 0.95rem;
  }

  .chrome {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(to bottom, rgb(0 0 0 / 0.45), transparent 22%, transparent 75%, rgb(0 0 0 / 0.55));
  }

  .close,
  .round {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: #fff;
    pointer-events: auto;
    cursor: pointer;
  }

  .close {
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    left: max(12px, env(safe-area-inset-left));
  }

  .corner {
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    right: max(12px, env(safe-area-inset-right));
    display: flex;
    gap: 4px;
  }

  /* The dice's scopes: a column at the right under the corner's buttons, alike (as tall, as round, as spaced), each a
     roll; open till the dice is tapped again. */
  .dice-row {
    position: absolute;
    top: calc(max(12px, env(safe-area-inset-top)) + 52px);
    right: max(12px, env(safe-area-inset-right));
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    max-height: calc(100% - 140px);
    overflow-y: auto;
  }
  .dice-row button {
    height: 48px;
    padding: 0 16px;
    border: 0;
    border-radius: 24px;
    background: transparent;
    color: #fff;
    font: inherit;
    pointer-events: auto;
    cursor: pointer;
  }
  .dice-row button:active {
    background: rgb(255 255 255 / 0.18);
  }
  /* Each scope with its pin beside it: a pinned scope is kept by every roll (its name dimmed, its pin lit). */
  .dice-row .scope {
    display: flex;
    align-items: center;
  }
  .dice-row .pin {
    width: 48px;
    padding: 0;
    display: grid;
    place-items: center;
    opacity: 0.45;
  }
  .dice-row .pin.on {
    opacity: 1;
    background: rgb(255 255 255 / 0.18);
  }
  .dice-row .held > button:first-child {
    opacity: 0.5;
  }

  .round.on {
    background: rgb(255 255 255 / 0.18);
  }

  .close:focus-visible,
  .round:focus-visible {
    background: rgb(255 255 255 / 0.12);
  }

  /* A touch screen keeps a tapped button "hovered": the circle only under a mouse. */
  @media (hover: hover) {
    .close:hover,
    .round:hover {
      background: rgb(255 255 255 / 0.12);
    }
  }

  /* The graph builder (decision 0056): beside the panel where there is room, the same height. */
  .graph {
    position: absolute;
    top: calc(max(12px, env(safe-area-inset-top)) + 56px);
    left: max(12px, env(safe-area-inset-left));
    right: calc(max(12px, env(safe-area-inset-right)) + 512px);
    bottom: max(12px, env(safe-area-inset-bottom));
    overflow: hidden;
    border-radius: 14px;
    background: rgb(14 16 21 / 0.86);
    pointer-events: auto;
  }

  /* The graph alone on a narrow screen: the whole page under the sheet. */
  .graph.whole {
    right: max(12px, env(safe-area-inset-right));
  }

  /* The panel as a sheet over the graph: from below, as tall as its view needs. */
  .panel.sheet {
    top: auto;
    left: 0;
    right: 0;
    bottom: 0;
    width: auto;
    max-height: calc(100% - max(12px, env(safe-area-inset-top)) - 56px);
    padding-bottom: max(10px, env(safe-area-inset-bottom));
    border-radius: 16px 16px 0 0;
    box-shadow: 0 -8px 24px rgb(0 0 0 / 0.4);
  }

  /* The panel's own close button: at its top right, staying there as it scrolls. */
  .panel-close {
    position: sticky;
    top: 0;
    float: right;
    z-index: 1;
    width: 36px;
    height: 36px;
    margin: 0 -8px 0 8px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 18px;
    background: rgb(20 20 24 / 0.92);
    color: #fff;
    cursor: pointer;
  }

  .panel {
    position: absolute;
    top: calc(max(12px, env(safe-area-inset-top)) + 56px);
    right: max(12px, env(safe-area-inset-right));
    bottom: max(12px, env(safe-area-inset-bottom));
    width: min(500px, calc(100% - 24px));
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgb(255 255 255 / 0.25) transparent;
    box-sizing: border-box;
    padding: 8px 16px 10px;
    border-radius: 14px;
    /* No blur behind it: blurring the moving picture under it every frame cost a tablet more than the picture did. */
    background: rgb(20 20 24 / 0.92);
    color: #fff;
    pointer-events: auto;
    touch-action: pan-y;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 6px;
  }

  .fps {
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.6);
  }

  .reset {
    min-height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 18px;
    background: rgb(255 255 255 / 0.1);
    color: #fff;
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
  }


  .track {
    position: absolute;
    left: max(24px, env(safe-area-inset-left));
    right: 192px;
    bottom: max(24px, env(safe-area-inset-bottom));
    display: grid;
    gap: 2px;
    color: #fff;
    text-shadow: 0 1px 6px rgb(0 0 0 / 0.6);
  }


  .sought {
    position: absolute;
    top: 50%;
    right: 12%;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgb(0 0 0 / 0.45);
    color: #fff;
    font-size: 1.1rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
    transform: translateY(-50%);
  }

  .sought.back {
    right: auto;
    left: 12%;
  }

  .transport {
    position: absolute;
    right: max(16px, env(safe-area-inset-right));
    bottom: max(16px, env(safe-area-inset-bottom));
    display: flex;
    gap: 8px;
  }

  .track strong {
    font-size: 1.15rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track span {
    color: rgb(255 255 255 / 0.75);
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
