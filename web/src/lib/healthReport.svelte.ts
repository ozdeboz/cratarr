// The health report and the files' problems (decision 0063), read once for the Health page's list and its panel
// (lib/HealthPanel) alike, and again on each visit to the page.
import { getFileProblems, getHealthReport, type FileProblem, type HealthFinding, type HealthItem } from './api';
import { DOMAINS, iconOf } from './health';
import type { IconName } from './Icon.svelte';

export const health = $state<{ findings: HealthFinding[]; loaded: boolean; problems: FileProblem[] | null; error: string | null }>({
  findings: [],
  loaded: false,
  problems: null,
  error: null,
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
    (r) => ((health.findings = r.findings), settle('report')),
    () => settle('report'),
  );
  getFileProblems().then(
    (p) => ((health.problems = p), (health.error = null), settle('files')),
    (e) => ((health.error = e instanceof Error ? e.message : 'Unexpected error'), settle('files')),
  );
}

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
  /** Its short name, the panel's breadcrumb (the owner, 2026-10-25: "Quality replacements", "Missing genres"). */
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

/** Every problem, the report's order (worst first); the files' kinds after their own findings would say the same. */
export function problems(): Problem[] {
  const out: Problem[] = health.findings
    .filter((f) => f.domain !== 'files')
    .map((f) => ({ key: f.key, icon: iconOf(f), urgent: f.level === 'critical', domain: DOMAINS[f.domain]?.name ?? f.domain, name: f.name ?? f.title, title: f.title, said: f.title, line: f.detail ?? null, finding: f, items: f.items ?? [], hint: null }));
  for (const k of FILE_KINDS) {
    const files = (health.problems ?? []).filter((p) => p.kind === k.kind);
    if (files.length)
      out.push({ key: `files-${k.kind}`, icon: DOMAINS.files.icon, urgent: k.kind === 'broken', domain: DOMAINS.files.name, name: health.findings.find((f) => f.key === `files:${k.kind}`)?.name ?? k.title, title: k.title, said: `${files.length.toLocaleString()} ${files.length === 1 ? 'file' : 'files'}${k.title.slice('Files'.length)}`, line: `${files.length} ${files.length === 1 ? 'file' : 'files'}`, finding: null, items: files.map(fileItem), hint: k.hint });
  }
  return out;
}
