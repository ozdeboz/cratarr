// The health report and the files' problems (decision 0063), read once for the Health page's list and its panel
// (lib/HealthPanel) alike, and again on each visit to the page.
import { getFileProblems, getHealthReport, type FileProblem, type HealthFinding } from './api';

export const health = $state<{ findings: HealthFinding[]; loaded: boolean; problems: FileProblem[] | null; error: string | null }>({
  findings: [],
  loaded: false,
  problems: null,
  error: null,
});

export function loadHealth() {
  getHealthReport().then(
    (r) => ((health.findings = r.findings), (health.loaded = true)),
    () => (health.loaded = true),
  );
  getFileProblems().then(
    (p) => ((health.problems = p), (health.error = null)),
    (e) => (health.error = e instanceof Error ? e.message : 'Unexpected error'),
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
