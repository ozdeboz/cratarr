// One way to change files (decision 0092; the owner, 2026-11-01: a review sheet before, the record's state line, one
// list of runs), the words and the merging, pure so the tests take them as they are.
import type { FileField, FileRun, FilesState } from './api';

/** A plan as its crumb names it (what proposes it). */
const PLANS: Record<string, string> = { types: 'Types and years' };
export const planTitle = (kind: string) => PLANS[kind] ?? 'Changes to review';

/** A field as the review sheet names it. */
const FIELDS: Record<string, string> = { releasetype: 'Type', date: 'Date', genre: 'Genre' };
export const fieldLabel = (field: string) => FIELDS[field] ?? field.charAt(0).toUpperCase() + field.slice(1);

/** A field's change as the sheet says it: values added (+), taken away (struck), or one set of them for another. */
export function changeOf(f: Pick<FileField, 'before' | 'after'>): { gone: string[]; came: string[]; kind: 'add' | 'remove' | 'set' } {
  const gone = f.before.filter((v) => !f.after.includes(v));
  const came = f.after.filter((v) => !f.before.includes(v));
  return { gone, came, kind: !gone.length ? 'add' : !came.length ? 'remove' : 'set' };
}

const time = (at: number) => new Date(at * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

/** A kept run (`r…`) under way again is being undone (its Undo runs as a write does, a record at a time). */
const undoing = (run: string | null | undefined) => !!run?.startsWith('r');

/** A run's line in the list: under way (writing, or its Undo), waiting, or how it ended and when. */
export function runLine(r: FileRun): string {
  if (r.state === 'waiting') return undoing(r.id) ? 'Waiting to undo' : 'Waiting';
  if (r.state === 'writing') return `${undoing(r.id) ? 'Undoing' : 'Writing'} ${r.done.toLocaleString()} of ${r.of.toLocaleString()}`;
  const when = r.at ? time(r.at) : '';
  if (r.undone) return ['Undone', when].filter(Boolean).join(' · ');
  const records = asideOf(r.failed).length;
  const aside = records ? `${records.toLocaleString()} set aside` : null;
  return [aside, when].filter(Boolean).join(' · ');
}

export type SetAside = { album: string; title?: string; artist?: string | null; files: number; why: string[] };
/** What a run set aside, a row per record (the owner, 2026-11-01: it listed each file, by the record's id): how many of
 *  its files, and each reason once, in the order they came. */
export function asideOf(failed: FileRun['failed']): SetAside[] {
  const by = new Map<string, SetAside>();
  for (const f of failed) {
    const r = by.get(f.album) ?? { album: f.album, title: f.title, artist: f.artist, files: 0, why: [] };
    if (f.file) r.files++;
    if (!r.why.includes(f.why)) r.why.push(f.why);
    r.title ??= f.title;
    r.artist ??= f.artist;
    by.set(f.album, r);
  }
  return [...by.values()];
}

/** The list's groups: Now (writing and waiting), then each day newest first ("Today", "Yesterday", a weekday within
 *  the week, else a date). */
export function runGroups(runs: readonly FileRun[], now = Date.now()): { title: string; runs: FileRun[] }[] {
  const day = (t: number) => {
    const d = new Date(t);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
  };
  const today = day(now);
  const name = (t: number) => {
    const d = day(t);
    const ago = Math.round((today - d) / 86_400_000);
    if (ago <= 0) return 'Today';
    if (ago === 1) return 'Yesterday';
    if (ago < 7) return new Date(d).toLocaleDateString(undefined, { weekday: 'long' });
    return new Date(d).toLocaleDateString(undefined, { day: 'numeric', month: 'long' });
  };
  const out: { title: string; runs: FileRun[] }[] = [];
  const live = runs.filter((r) => r.state !== 'done');
  if (live.length) out.push({ title: 'Now', runs: live });
  for (const r of runs.filter((x) => x.state === 'done').sort((a, b) => (b.at ?? 0) - (a.at ?? 0))) {
    const title = name((r.at ?? 0) * 1000);
    const g = out.find((x) => x.title === title);
    if (g) g.runs.push(r);
    else out.push({ title, runs: [r] });
  }
  return out;
}

/** A run's status into the list: in place by its id; a kept one (`r…`) taking the place of the live one it was (the
 *  same `what`, the same `at` where the live one had one). */
export function withRun(runs: readonly FileRun[], s: FileRun): FileRun[] {
  let list = runs.filter((r) => r.id !== s.id);
  if (s.id.startsWith('r')) list = list.filter((r) => !(r.id.startsWith('l') && r.what === s.what && (r.at === undefined || r.at === s.at)));
  return [s, ...list];
}

/** The record's state line (its panel, under its title): its words, and what a tap does there. */
export function stateLine(s: FilesState | null | undefined): { text: string; tone: 'warn' | null; act: 'review' | 'undo' | null; progress: number | null } | null {
  if (!s) return null;
  switch (s.state) {
    case 'differs':
      return { text: 'Its files differ from Cratarr', tone: null, act: 'review', progress: null };
    case 'waiting':
      return { text: undoing(s.run) ? 'Waiting to undo its files' : 'Waiting to write its files', tone: null, act: null, progress: null };
    case 'writing':
      return { text: `${undoing(s.run) ? 'Undoing' : 'Writing'} its files${s.of ? `, ${s.done ?? 0} of ${s.of}` : ''}`, tone: null, act: null, progress: s.of ? (s.done ?? 0) / s.of : 0 };
    case 'written':
      return { text: 'Its files written', tone: null, act: s.run ? 'undo' : null, progress: null };
    case 'changed':
      return { text: 'A file changed since it was planned', tone: 'warn', act: 'review', progress: null };
  }
  return null;
}
