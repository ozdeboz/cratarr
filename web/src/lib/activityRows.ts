// Health › Activity (RN item 6): every job Cratarr runs or that waits, from the server's runner, a row each: what it does now and
// to what, how far, what it waits for and until when, and whose. Its words are the jobs' one table (shared/states.json, the server's
// `runner::words`; tests/health-activity.test.ts holds the two equal), a time said by the page's `when()`. Pure.

/** A job as `/api/activity` and the live channel's `activity` give it. */
export type ActivityJob = {
  id: string;
  kind: string;
  state: 'waiting' | 'scheduled' | 'running' | 'retrying' | 'done' | 'failed' | 'cancelled';
  what: string;
  detail?: string;
  done?: number;
  of?: number;
  unit?: string;
  waits?: { what: string; until?: number | null };
  at?: number;
  tries?: number;
  whose?: string;
  started: number;
  finished?: number;
};

/** The jobs' words by their key (shared/states.json's `words`). */
export const WORDS = {
  waits_for_room: 'Waits for room · {used} of {room} GB',
  waits_for_space: 'Waits for space · {free} GB free',
  waits_for_release: 'Waits for its release · {day}',
  not_found_yet: 'Not found yet · again in {in}',
  stuck: 'Stuck: {why} · tried again at {at}',
  paused_until: 'Paused until {until}',
  percent: '{percent}%',
  waits_to_be_checked: 'Waits to be checked',
  checked: '{done} of {of} checked',
  files: '{done} of {of} files',
  count: '{done} of {of}',
  waits_for_turn: 'Waits for its turn to {what}',
  waits_for_memory: 'Waits for memory',
  resting: 'Resting',
  background_off: 'Background reading is paused',
  asked_for: 'asked for',
  catching_up: 'catching up',
  again_at: 'Tried again at {at}',
  owed: 'Lidarr is told again at {at}',
} as const;

/** A word, its parts filled. */
export const say = (key: keyof typeof WORDS, parts: Record<string, string> = {}) => WORDS[key].replace(/\{(\w+)\}/g, (m, k: string) => parts[k] ?? m);

const number = (n: number) => n.toLocaleString('en-US');

/** How far it is ("1,204 of 3,580", "4 of 9 files"), where it counts. */
export function farOf(j: ActivityJob): string | null {
  if (j.done == null || j.of == null) return null;
  return say(j.unit === 'files' ? 'files' : 'count', { done: number(j.done), of: number(j.of) });
}

/** What it waits for and until when ("Discogs · Paused until 14:05" says only the time; a hold its own words first). */
export function waitsOf(j: ActivityJob, when: (s: number) => string): string | null {
  if (!j.waits) return null;
  const parts = [j.waits.what, j.waits.until ? say('paused_until', { until: when(j.waits.until) }) : ''].filter(Boolean);
  return parts.length ? parts.join(' · ') : null;
}

/** Its whole line, as the server's `Row::words` says it: what, to what, how far, what it waits for. */
export function lineOf(j: ActivityJob, when: (s: number) => string): string {
  return [j.what, j.detail, farOf(j), waitsOf(j, when)].filter(Boolean).join(' · ');
}

/** Its row on Health (lib/settings/Item): its work as the name; to what, what it waits for and whose as the line; how far as the
 *  value and the bar. */
export function activityItem(j: ActivityJob, when: (s: number) => string) {
  const whose = j.whose ? `${j.whose}'s` : null;
  return {
    key: j.id,
    name: j.what,
    line: [j.detail, waitsOf(j, when), whose].filter(Boolean).join(' · ') || null,
    value: farOf(j),
    progress: j.done != null && j.of ? j.done / j.of : null,
    waiting: !!j.waits,
  };
}

/** The list after a patch: a job told again replaces itself, one that ended leaves. */
export function patched(jobs: ActivityJob[], j: ActivityJob): ActivityJob[] {
  if (j.finished != null || ['done', 'failed', 'cancelled'].includes(j.state)) return jobs.filter((x) => x.id !== j.id);
  const at = jobs.findIndex((x) => x.id === j.id);
  return at < 0 ? [...jobs, j] : jobs.map((x, i) => (i === at ? j : x));
}
