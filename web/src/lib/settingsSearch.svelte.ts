// Settings' search (the owner, 2026-10-24: in the top right, left of the info button): which parts hold a row, a group
// or a choice by the words searched. The words are read from the parts themselves (their rows' names, their groups'
// titles, their choices' labels), so the search never falls behind them; loaded the first time something is searched.
import { FACTS } from './facts';
import { HOME_PARTS, PROFILE_ROWS } from './prefs.svelte';
import { matches } from './search';
import { IMPORT_WHAT } from './settings/editor.svelte';
import type { SettingsSection } from './settingsSections.svelte';

/** The lists a part shows that are kept in another file (the owner, 2026-10-25: Home's sections, the profile's rows…
 *  found by their labels as every other row is). */
const SHOWN: Partial<Record<SettingsSection, readonly { label: string }[]>> = {
  content: [...HOME_PARTS, ...PROFILE_ROWS],
  player: FACTS,
  import: IMPORT_WHAT,
};

/** Each file's part: a part's own file, and those a part shows (its groups of their own). */
const PARTS: Record<string, SettingsSection> = {
  'AccountPart': 'account',
  'AuthPart': 'auth',
  'DirectoryPart': 'auth',
  'ConnectionsPart': 'connections',
  'HomeAssistantPart': 'connections',
  'ListeningPart': 'tracking',
  'RadioPart': 'radio',
  'TrackingPart': 'tracking',
  'DevicePart': 'device',
  'DiagnosticsPart': 'diagnostics',
  'AppearancePart': 'appearance',
  'AppearancePicker': 'appearance',
  'LayoutPart': 'layout',
  'PlayerPart': 'player',
  'PlayerRowList': 'player',
  'ContentPart': 'content',
  'ImportPart': 'import',
  'LibrariesPart': 'libraries',
  'NetworkPart': 'network',
  'SyncPart': 'sync',
  'TastePart': 'taste',
  'UsersPart': 'users',
  'PlayersSettings': 'players',
};

const SOURCES = import.meta.glob(['./settings/*Part.svelte', './settings/PlayerRowList.svelte', './settings/AppearancePicker.svelte', './PlayersSettings.svelte'], {
  query: '?raw',
  import: 'default',
}) as Record<string, () => Promise<string>>;

/** A part's words: its rows', groups' and choices' plain names. */
export const settingsWords = $state({ by: null as Partial<Record<SettingsSection, string[]>> | null });

let loading: Promise<void> | null = null;
/** Reads the parts' words, once. */
export function loadSettingsWords(): Promise<void> {
  loading ??= (async () => {
    const by: Partial<Record<SettingsSection, string[]>> = {};
    await Promise.all(
      Object.entries(SOURCES).map(async ([path, load]) => {
        const part = PARTS[path.split('/').pop()!.replace('.svelte', '')];
        if (!part) return;
        const text = await load();
        // A row's, a group's or a choice's text, written in the markup (`label="Dark"`) or in the code (`label: 'Dark'`,
        // a choice's; the owner, 2026-10-25: "dark and light should be labels, no?").
        const found = [...text.matchAll(/\b(?:name|title|label)\s*(?:=|:)\s*(?:"([^"{}]{2,60})"|'([^'{}]{2,60})')/g)].map((m) => (m[1] ?? m[2]).trim());
        by[part] = [...new Set([...(by[part] ?? []), ...found])];
      }),
    );
    for (const [part, list] of Object.entries(SHOWN) as [SettingsSection, readonly { label: string }[]][]) {
      by[part] = [...new Set([...(by[part] ?? []), ...list.map((x) => x.label)])];
    }
    settingsWords.by = by;
  })();
  return loading;
}

/** The names in `part` that hold every word searched, and whether the part's own title does. */
export function settingsMatches(part: SettingsSection, title: string, words: string[]): { title: boolean; rows: string[] } {
  // As every search reads (lib/search, decision 0072): `words` read by it, each a word's start.
  const has = (s: string) => matches(words, s);
  return { title: has(title), rows: (settingsWords.by?.[part] ?? []).filter(has) };
}
