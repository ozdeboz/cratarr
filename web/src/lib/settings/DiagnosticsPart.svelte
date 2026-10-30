<script lang="ts">
  // Diagnostics (decision 0045): what happened, as Cratarr keeps it, laid out as Finamp lays out its logs (the owner,
  // 2026-10-17: "we learn from the way Finamp presents settings and deeper settings"): one switch on top for more
  // detail, a day long, with the areas one by one under it only while it is on; each player's link as Cratarr knows it
  // and as the player last told; then the log, newest first, by player, area and level (its filter in a sheet from the
  // group's heading). Groups of rows (the settings' North Star, 2026-10-19). The file for a bug report is the bar's
  // Download. The log as it is, no prose.
  import { offers } from '../capabilities.svelte';
  import { getAbout, getLog, getLogLevels, getPlayers, missingCall, restartServer, serverAnswers, setLogLevels, type LogLevels, type LogRow, type PlayOnPlayer } from '../api';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import { toast } from '../toast.svelte';
  import { when } from './when';
  import ChoiceList from '../ChoiceList.svelte';
  import Icon from '../Icon.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';

  const AREA_NAMES: Record<string, string> = {
    playon: 'Play On',
    syncplay: 'Sync play',
    signin: 'Sign-in',
    library: 'Library',
    subsonic: 'Subsonic',
    imports: 'Imports',
    server: 'Server',
  };
  const areaName = (a: string) => AREA_NAMES[a] ?? a;
  const PAGE = 200;

  // Restart (the owner, 2026-10-26: "without command line"): asked first; then it stops, and the page waits for it to
  // answer again (a minute at most before it says so), then reads itself again.
  let started = $state<number | null>(null);
  getAbout().then(
    (a) => (started = a.started),
    () => {},
  );
  let restartAsk = $state(false);
  let restarting = $state(false);
  async function restart() {
    restarting = true;
    try {
      await restartServer();
    } catch (e) {
      restarting = false;
      return toast.fail(missingCall(e) ? null : e, missingCall(e) ? "This server can't restart itself yet" : "Cratarr didn't restart");
    }
    // Down first (it stops half a second after answering), then back.
    const t0 = Date.now();
    let down = false;
    while (Date.now() - t0 < 180_000) {
      await new Promise((r) => setTimeout(r, 1500));
      const up = await serverAnswers();
      if (!up) down = true;
      else if (down || Date.now() - t0 > 8000) return location.reload();
    }
    restarting = false;
    toast.fail(null, "Cratarr hasn't come back yet");
  }

  let players = $state<PlayOnPlayer[]>([]);
  let rows = $state<LogRow[]>([]);
  let levels = $state<LogLevels | null>(null);
  let place = $state('');
  let area = $state('');
  let level = $state('');
  let more = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** More detail lasts this long, then goes back by itself. */
  const HOURS = 24;

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  // Only the latest load's answer is shown (an older one may come after it).
  let asked = 0;
  async function load(next = false) {
    const mine = ++asked;
    busy = true;
    error = null;
    try {
      const before = next ? rows.at(-1)?.id : undefined;
      const r = await getLog({ place, area, level, before, limit: PAGE });
      if (mine !== asked) return;
      rows = next ? [...rows, ...r.rows] : r.rows;
      more = r.rows.length === PAGE;
    } catch (e) {
      if (mine === asked) failed(e);
    } finally {
      if (mine === asked) busy = false;
    }
  }

  // Again whenever what is shown changes.
  $effect(() => {
    void [place, area, level];
    load();
  });
  getPlayers().then(
    (r) => (players = r.players),
    () => (players = []),
  );
  getLogLevels().then(
    (l) => (levels = l),
    () => (levels = null),
  );

  async function setLevels(now: Record<string, string>) {
    error = null;
    try {
      levels = await setLogLevels(now, HOURS);
    } catch (e) {
      failed(e);
    }
  }

  /** One area more (or less) than the rest. */
  function raise(a: string, to: string) {
    if (!levels) return;
    const now = { ...levels.now };
    if (to === 'info') delete now[a];
    else now[a] = to;
    setLevels(now);
  }

  const detailed = $derived(!!levels && Object.keys(levels.now).length > 0);
  /** More detail: every area, or back to normal. */
  const detail = (on: boolean) => levels && setLevels(on ? Object.fromEntries(levels.areas.map((a) => [a, 'debug'])) : {});
  offerReset(
    () => (detailed ? 'More detail back to off, every area to normal.' : null),
    () => setLevels({}),
  );

  const today = new Date().toDateString();
  /** The time, with the day when it is not today. */
  function time(ms: number, seconds = true) {
    const d = new Date(ms);
    const t = d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', ...(seconds ? { second: '2-digit' } : {}) });
    return d.toDateString() === today ? t : `${d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })} ${t}`;
  }

  /** A row's details as `key value`, times shown as times. */
  function details(f: Record<string, unknown>) {
    return Object.entries(f).map(([k, v]) => {
      const n = typeof v === 'number' && (k === 'until' || k.endsWith('_at') || k === 'connected_since') && v > 1e12 ? time(v) : v;
      return `${k} ${typeof n === 'object' ? JSON.stringify(n) : n}`;
    });
  }

  const places = $derived(['server', ...players.map((p) => p.name)]);

  const LEVELS = [
    { value: 'info', label: 'Normal' },
    { value: 'debug', label: 'More' },
    { value: 'trace', label: 'Everything' },
  ];
  const levelName = (v: string) => LEVELS.find((l) => l.value === v)?.label ?? v;
  /** A player's link in a line: what failed, how long it waits, or since when it is offline. */
  function linkLine(p: PlayOnPlayer) {
    const l = p.link;
    const parts: string[] = [];
    if (l?.own?.failed) parts.push(`${l.own.failed} failed${l.own.last_error ? `: ${l.own.last_error}` : ''}`);
    if (l?.own?.wait_until) parts.push(`waiting until ${time(l.own.wait_until, false)}${l.own.wait_why ? ` (${l.own.wait_why})` : ''}`);
    if (!p.online) parts.push(l?.offline_since ? `Offline since ${time(l.offline_since * 1000, false)}` : 'Offline', ...(l?.ended ? [l.ended] : []));
    return parts.join(' · ') || null;
  }
  let raising = $state<string | null>(null);
  let filtering = $state(false);
  const filtered = $derived(!!place || !!area || !!level);
</script>

<!-- Only where the server can restart itself (0.11, lib/capabilities: server_restart; a server before it, as before). -->
{#if offers('server_restart')}<Group title="Server">
  <Item
    name="Restart Cratarr"
    line={restarting ? 'Restarting…' : started ? `Running since ${when(started)}` : null}
    busy={restarting}
    disabled={restarting}
    onclick={() => (restartAsk = true)}
  />
</Group>{/if}
{#if restartAsk}
  <ConfirmSheet
    title="Restart Cratarr?"
    text="It stops for everyone and every player, and is back in about half a minute; longer when it rebuilds the library."
    label="Restart"
    icon="reset"
    onconfirm={restart}
    onclose={() => (restartAsk = false)}
  />
{/if}

{#if levels}
  <Group>
    <Item
      name="More detail"
      line={detailed && levels.until ? `Until ${time(levels.until, false)}` : null}
      more="Cratarr logs more of what happens, for a day, and then goes back by itself. While it is on, each area can be set on its own."
      toggle={{ checked: detailed, onchange: detail }}
    />
    {#if detailed}
      {#each levels.areas as a (a)}
        <Item name={areaName(a)} value={levelName(levels.now[a] ?? 'info')} onclick={() => (raising = a)} />
      {/each}
    {/if}
  </Group>
{/if}

{#if players.length}
  <Group title="Devices">
    {#each players as p (p.name)}
      {@const l = linkLine(p)}
      <Item lead={p.online ? 'on' : 'off'} name={p.name} line={l} tone={p.link?.own?.failed ? 'warn' : null} wrap />
    {/each}
  </Group>
{/if}

<Group title="Log">
  {#snippet tail()}
    <button class="filter" class:on={filtered} aria-label="Filter" title="Filter" onclick={() => (filtering = true)}><Icon name="filter" size={18} /></button>
  {/snippet}
  {#if error}<Item lead="bad" name={error} />{/if}
  {#each rows as r (r.id)}
    <div class="entry {r.level}">
      <span class="msg">{r.what}{#if r.count > 1}<span class="count"> ×{r.count}, last {time(r.last)}</span>{/if}</span>
      <span class="head">{[time(r.at), r.place, areaName(r.area), r.origin === 'device' ? 'its log' : null].filter(Boolean).join(' · ')}</span>
      {#if Object.keys(r.fields).length}<span class="fields">{details(r.fields).join(' · ')}</span>{/if}
    </div>
  {:else}
    {#if !busy}<Item name="Nothing logged" />{/if}
  {/each}
  {#if more}<Item name="More" kind="add" disabled={busy} busy={busy} onclick={() => load(true)} />{/if}
</Group>

{#if raising && levels}
  {@const a = raising}
  <ChoiceSheet title={areaName(a)} options={LEVELS} value={levels.now[a] ?? 'info'} onchange={(v) => raise(a, v)} onclose={() => (raising = null)} />
{/if}
{#if filtering}
  <MenuSheet label="Filter" icon="filter" title="Filter" onclose={() => (filtering = false)}>
    <div class="filters">
      <h4>Where</h4>
      <ChoiceList label="Where" options={[{ value: '', label: 'Everywhere' }, ...places.map((p) => ({ value: p, label: p === 'server' ? 'Server' : p }))]} value={place} onchange={(v) => (place = v)} />
      <h4>Area</h4>
      <ChoiceList
        label="Area"
        options={[{ value: '', label: 'All areas' }, ...(levels?.areas ?? Object.keys(AREA_NAMES)).map((a) => ({ value: a, label: areaName(a) }))]}
        value={area}
        onchange={(v) => (area = v)}
      />
      <h4>Level</h4>
      <ChoiceList
        label="Level"
        options={[
          { value: '', label: 'All' },
          { value: 'warn', label: 'Warnings' },
          { value: 'error', label: 'Errors' },
        ]}
        value={level}
        onchange={(v) => (level = v)}
      />
    </div>
  </MenuSheet>
{/if}

<style>
  /* A log entry: what happened, then when, where and in which area, then its details; warnings and errors coloured. */
  .entry {
    display: grid;
    gap: 2px;
    padding: 10px 14px 10px 16px;
  }

  .msg {
    font-size: 15px;
    overflow-wrap: anywhere;
  }

  .entry.warn .msg {
    color: var(--warn-ink);
  }

  .entry.error .msg {
    color: var(--danger-ink);
  }

  .count,
  .head {
    color: var(--muted);
    font-size: 13px;
  }

  .fields {
    color: var(--muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .filter {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin: -6px 0;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .filter:hover,
  .filter:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .filter.on {
    color: var(--accent-ink);
  }

  .filters {
    padding: 0 8px 12px;
    overflow-y: auto;
  }

  h4 {
    margin: 12px 16px 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }
</style>
