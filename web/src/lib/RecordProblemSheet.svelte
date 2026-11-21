<script lang="ts">
  // A record's health line opened in place (F28; the owner's "no U-turn"): a sheet over the record's page, titled by the line's own
  // words, its level under the title with its (i), and for each of the finding's items what can be done (lib/HealthItemTools), as
  // Health's row shows it, never a Go to the record it is over. A tool that opens a sheet of its own takes this one's place;
  // closed, the record's page shows as it was.
  import { wantAlbum, type HealthItem, type RecordFinding } from './api';
  import { releaseOf, type ReleaseTrack } from './replaceTrack';
  import { sayWantFailed } from './albums.svelte';
  import { wantOf } from './wantPath';
  import { me } from './me.svelte';
  import { findsTracks } from './capabilities.svelte';
  import { FIXABLE } from './fixes.svelte';
  import { review } from './fileRuns.svelte';
  import { LEVEL, fullOf } from './healthTools';
  import RecordMenu from './sheet/Record.svelte';
  import HealthItemTools from './HealthItemTools.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MoreInfo from './MoreInfo.svelte';
  import Notice from './Notice.svelte';
  import ReplaceTrackSheet from './ReplaceTrackSheet.svelte';

  let {
    finding,
    problem,
    title,
    here,
    failed = null,
    onretry,
    onclose,
    onchanged,
  }: {
    /** The finding as the record's health reads now (none: gone, and the host closes this). */
    finding: RecordFinding;
    /** Its line's key (a files kind's `files-<kind>`). */
    problem: string;
    /** The line's own words. */
    title: string;
    /** The record it is over. */
    here: string;
    /** Why the record's health could not be read again. */
    failed?: string | null;
    onretry: () => void;
    onclose: () => void;
    /** Something done here: the record's health read again. */
    onchanged: () => void;
  } = $props();

  const finds = $derived(me.admin && findsTracks());
  const findingKey = $derived(problem.startsWith('files-') ? `files:${problem.slice('files-'.length)}` : problem);
  const fixable = $derived(me.admin && FIXABLE.has(findingKey));
  const keyOf = (x: HealthItem) => (x.track ? `t/${x.track}` : x.album ? `a/${x.album.join('/')}` : `n/${x.title}`);

  // The missing tracks as their release has them (each its recording, to look for), read as the sheet opens.
  let releases = $state<Record<string, ReleaseTrack[]>>({});
  $effect(() => {
    let current = true;
    for (const x of finding.items) {
      if (!x.missing?.length || x.album?.[0] !== 'cratarr' || !finds) continue;
      const key = keyOf(x);
      releaseOf(x.album[0], x.album[1]).then(
        (r) => current && (releases = { ...releases, [key]: r.tracks.filter((t) => !t.have).sort((a, b) => a.disc - b.disc || a.position - b.position) }),
        () => {},
      );
    }
    return () => (current = false);
  });

  let asking = $state(false);
  let askedFull = $state<string | null>(null);
  async function askFull(x: HealthItem) {
    const key = fullOf(x);
    if (!key || asking) return;
    asking = true;
    try {
      const record = x.album?.[0] === 'cratarr' ? x.album[1] : null;
      await wantAlbum(wantOf({ record, release_group: x.release_group, artist: (x.line ?? '').split(' · ')[0], album: x.title }));
      askedFull = key;
      onchanged();
    } catch (e) {
      sayWantFailed(e);
    } finally {
      asking = false;
    }
  }

  // A tool's own sheet, in this one's place (F28.4).
  let searchOf = $state<{ album: string; title: string; recording: string } | null>(null);
  let betterOf = $state<{ source: string; album: string; track: string } | null>(null);
  let coverOf = $state<{ id: string; title: string } | null>(null);
  const elsewhere = $derived(!!(searchOf || betterOf || coverOf));
  // Review the fixes opens the one review sheet (`review.thing`): it takes this one's place.
  $effect(() => {
    if (review.thing) onclose();
  });
  const done = () => ((searchOf = betterOf = coverOf = null), onchanged(), onclose());
</script>

{#if searchOf}
  <ReplaceTrackSheet source="cratarr" album={{ id: searchOf.album, title: searchOf.title }} start={searchOf.recording} onclose={done} />
{:else if betterOf}
  <ReplaceTrackSheet source={betterOf.source} album={{ id: betterOf.album, title: '' }} replace={{ source: betterOf.source, id: betterOf.track }} onclose={done} />
{:else if coverOf}
  <RecordMenu id={coverOf.id} from="problem" title={coverOf.title} start="cover" onclose={done} />
{:else if !elsewhere}
  <MenuSheet label={title} {title} {onclose}>
    <div class="problem">
      <p class="level" class:urgent={finding.level === 'critical'}>
        <span>{LEVEL[finding.level]}</span>{#if finding.detail}<MoreInfo label={title} text={finding.detail} />{/if}
      </p>
      {#if failed}<Notice text={failed} {onretry} away="Health needs Cratarr." />{/if}
      {#each finding.items as x (keyOf(x))}
        <HealthItemTools
          {problem}
          item={x}
          {here}
          {finds}
          {fixable}
          {findingKey}
          tracks={releases[keyOf(x)] ?? null}
          {asking}
          {askedFull}
          flush
          onaskfull={askFull}
          onsearch={(s) => (searchOf = s)}
          onbetter={(b) => (betterOf = b)}
          oncover={(c) => (coverOf = c)}
          ongone={() => (onchanged(), onclose())}
        />
      {/each}
    </div>
  </MenuSheet>
{/if}

<style>
  .problem {
    padding: 0 clamp(12px, 2vw, 24px) 8px;
  }

  .level {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    color: var(--muted);
  }

  .level.urgent {
    color: var(--danger, var(--text));
  }
</style>
