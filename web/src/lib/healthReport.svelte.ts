// The health report and the files' problems (decision 0063), read once for the Health page's list and its panel
// (lib/HealthPanel) alike, and again on each visit to the page.
import { getFileProblems, getHealthReport, type FileProblem, type HealthFinding, type HealthItem, type RecordFinding } from './api';
import { whenChanged } from './dataChanged';
import { type Area, areaOf, DOMAINS, iconOf } from './health';
import type { IconName } from './Icon.svelte';

/** `error`: the files' problems could not be read; `reportError`: the report could not (the owner, 2026-11-02: errors
 *  are never shown as nothing, so never "All good." after either failed). `checked`: when each domain was last checked
 *  (epoch s), as the report says. */
export const health = $state<{ findings: HealthFinding[]; checked: Record<string, number>; loaded: boolean; problems: FileProblem[] | null; error: string | null; reportError: string | null }>({
  findings: [],
  checked: {},
  loaded: false,
  problems: null,
  error: null,
  reportError: null,
});

/** Loaded once both answers are in (the owner, 2026-10-23): never "All good." before the files' problems come. */
let settled = { report: false, files: false };
const settle = (which: keyof typeof settled) => {
  settled[which] = true;
  if (settled.report && settled.files) health.loaded = true;
};

export function loadHealth() {
  settled = { report: false, files: false };
  getHealthReport().then(
    (r) => ((health.findings = r.findings), (health.checked = r.checked ?? {}), (health.reportError = null), settle('report')),
    (e) => ((health.reportError = e instanceof Error ? e.message : 'Unexpected error'), settle('report')),
  );
  getFileProblems().then(
    (p) => ((health.problems = p), (health.error = null), settle('files')),
    (e) => ((health.error = e instanceof Error ? e.message : 'Unexpected error'), settle('files')),
  );
}

// Read again whenever what it reports on changed (lib/dataChanged): a fix written, a record kept or moved, a playlist
// changed; what was read stays shown meanwhile. Once read, kept true for the whole page.
whenChanged(['files', 'collection', 'incoming', 'playlists', 'words'], () => health.loaded && loadHealth());

/** The files' kinds of problem, each its own list in the Files part. */
export const FILE_KINDS: { kind: FileProblem['kind']; title: string; hint: string }[] = [
  { kind: 'broken', title: "Files that can't be read", hint: 'Not readable as audio at all. Usually a damaged or incomplete file; replace it.' },
  {
    kind: 'damaged_tag',
    title: 'Files with a damaged tag',
    hint: 'One tag field is broken and was skipped; the music and the other tags are fine. Saving the tags again in a tag editor fixes it.',
  },
  { kind: 'no_tags', title: 'Files without tags', hint: 'No artist, title or other tags at all.' },
  { kind: 'no_length', title: 'Files without a length', hint: "The audio's length could not be read." },
];

/** A problem, as Health lists it (the owner, 2026-10-23: "one card per problem"): a finding, or one kind of file problem
 *  with its files. `key` is its address in the panel (`health:<key>`). */
export type Problem = {
  key: string;
  icon: IconName;
  urgent: boolean;
  domain: string;
  /** Its area of Health (lib/health AREAS). */
  area: Area;
  /** Its name, the one it goes by everywhere: Health's list, the panel's breadcrumb, its picker, Up, global search (the
   *  owner, 2026-11-02: the picker said "Responses" and the breadcrumb "Slow responses"; "Shouldn't we use the short
   *  ones everywhere?"): the finding's thing where it has one, else its name. */
  name: string;
  /** What it is, on its card ("12 albums without a genre"; a file kind's "Files without tags"). */
  title: string;
  /** What it is in a sentence with how many, at the top of its panel (the owner, 2026-10-25). */
  said: string;
  line: string | null;
  finding: HealthFinding | null;
  /** What it is about, one by one: the server's items, or the files. */
  items: HealthItem[];
  hint: string | null;
};

/** A file as an item: its name, its album folder and what is wrong with it. */
const fileItem = (p: FileProblem): HealthItem => {
  const at = p.rel.lastIndexOf('/');
  return { title: p.rel.slice(at + 1), line: [p.rel.slice(0, Math.max(0, at)), p.detail].filter(Boolean).join(' · ') };
};

/** Every problem, the report's order (worst first); the files' kinds after their own findings would say the same.
 *  A kind of the files' is a problem wherever the report counts it, whether its files are listed yet or not: the list
 *  holds the first files of every kind together, and a kind past them was no problem at all, so Home's row for it
 *  opened "This is no longer a problem" (the owner, 2026-11-02). */
export function problems(): Problem[] {
  const out: Problem[] = health.findings
    .filter((f) => f.domain !== 'files')
    .map((f) => ({ key: f.key, icon: iconOf(f), urgent: f.level === 'critical', domain: DOMAINS[f.domain]?.name ?? f.domain, area: areaOf(f.domain), name: f.thing ?? f.name ?? f.title, title: f.title, said: f.title, line: f.detail ?? null, finding: f, items: f.items ?? [], hint: null }));
  const listed = health.problems ?? [];
  const counted = health.findings.filter((f) => f.domain === 'files').map((f) => f.key.split(':')[1]);
  const kinds = [...FILE_KINDS.map((k) => k.kind as string), ...counted.filter((k) => !FILE_KINDS.some((x) => x.kind === k))];
  for (const kind of kinds) {
    const k = FILE_KINDS.find((x) => x.kind === kind);
    const f = health.findings.find((x) => x.key === `files:${kind}`);
    const files = listed.filter((p) => p.kind === kind);
    // Its items: the files list's where it holds them, else the finding's own (cut off, sound breaking off, lengths
    // guessed come only with their finding; the owner, 2026-11-03: "8 tracks' lengths are guessed", none listed).
    const items = files.length ? files.map(fileItem) : (f?.items ?? []);
    const n = Math.max(f?.count ?? 0, items.length);
    if (!n) continue;
    const title = k?.title ?? f?.title ?? kind;
    const nFiles = (x: number) => `${x.toLocaleString()} ${x === 1 ? 'file' : 'files'}`;
    out.push({
      key: `files-${kind}`,
      icon: DOMAINS.files.icon,
      urgent: kind === 'broken',
      domain: DOMAINS.files.name,
      area: areaOf('files'),
      name: f?.thing ?? f?.name ?? title,
      title,
      said: k ? `${nFiles(n)}${k.title.slice('Files'.length)}` : (f?.title ?? title),
      line: items.length < n ? `${items.length.toLocaleString()} of ${nFiles(n)} listed` : nFiles(n),
      finding: null,
      items,
      hint: k?.hint ?? null,
    });
  }
  return out;
}

/** A record's own problem (the owner, 2026-11-02: its Health problems on its page, a line each under its title): the
 *  Health problem it is of (a file kind's as problems() names it, `files-…`), its line, and whether it is urgent.
 *  Missing tracks say how many (`missing`), the line the page has for them. */
export function recordProblem(f: RecordFinding): { key: string; text: string; urgent: boolean; missing: number } {
  const kind = f.key.startsWith('files:') ? f.key.slice('files:'.length) : null;
  const n = f.items.length;
  const missing = f.items.reduce((m, x) => m + (x.missing?.length ?? 0), 0);
  if (kind) {
    const k = FILE_KINDS.find((x) => x.kind === kind);
    return { key: `files-${kind}`, text: `${k?.title ?? f.thing ?? f.name}${n > 1 ? ` · ${n} tracks` : ''}`, urgent: kind === 'broken', missing: 0 };
  }
  const text = [f.thing ?? f.name, f.state].filter(Boolean).join(' ');
  return { key: f.key, text: text.charAt(0).toUpperCase() + text.slice(1), urgent: f.level === 'critical', missing };
}
/** A record narrowed to in Health's panel (`health:<problem>/<the record's subject>`): its server and id. */
export function recordIn(thing: string | null): { source: string; id: string } | null {
  const m = thing?.match(/^album:(.+)\/([^/]+)$/);
  return m ? { source: m[1], id: m[2] } : null;
}
