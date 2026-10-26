// The fixes Health offers for its library findings (0.9.7, the administrators' metadata workflow; the owner,
// 2026-10-26: Cratarr proposes, an administrator approves, written into the files with Undo): read per finding, and
// again every two seconds while MusicBrainz is still being asked (the server's `matching`). An item's page shows its
// album's (lib/HealthFixes); the finding's list offers every certain one in one tap.
import { applyFixes, getFixes, type Fix, type Fixes } from './api';
import { loadHealth } from './healthReport.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

/** The findings with fixes. */
export const FIXABLE = new Set(['library:year', 'library:genre', 'library:album-artist', 'library:musicbrainz', 'library:rules']);
export const FIELD_LABEL: Record<Fix['field'], string> = {
  year: 'Year',
  genre: 'Genre',
  albumartist: 'Album artist',
  musicbrainz: 'MusicBrainz release',
  cover: 'Cover',
  rule: 'Named by a rule',
};

export const healthFixes = $state<Record<string, Fixes>>({});
const waits = new Map<string, ReturnType<typeof setTimeout>>();

/** Reads a finding's fixes (and keeps reading while they are being matched). */
export function readFixes(finding: string) {
  clearTimeout(waits.get(finding));
  getFixes(finding).then(
    (f) => {
      healthFixes[finding] = f;
      if (f.matching) waits.set(finding, setTimeout(() => readFixes(finding), 2000));
    },
    () => {},
  );
}

/** An album's fixes in a finding. */
export const fixesOf = (finding: string, album: string) => healthFixes[finding]?.fixes.filter((f) => f.album === album) ?? [];

/** Applies fixes, says what came of it (with Undo), and reads Health and the finding's fixes again. */
export async function apply(which: { ids: string[] } | { certain: string }, finding: string, what: string): Promise<boolean> {
  try {
    const r = await applyFixes(which);
    const after = () => (loadHealth(), finding && readFixes(finding));
    if (r.applied) withUndo(r.failed.length ? `${what}; ${r.failed.length} failed.` : `${what}.`, r.undo, { after });
    else toast.fail(null, r.failed[0]?.why ?? 'Nothing was changed');
    after();
    return r.applied > 0;
  } catch (e) {
    toast.fail(e, 'Nothing was changed');
    return false;
  }
}
