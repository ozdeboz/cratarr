// What the global search finds in the browser itself (the owner, 2026-10-25: "Global search should also surface
// settings, and anything else from any other pages. It's a global search."): the pages, Settings' parts (by their
// names and their rows', lib/settingsSearch), Insights' views, the lists of Requests and Downloads. Each result's
// subject is the address that shows it, so a tap goes there (the panel opened on its part); the words searched stay,
// so the page or the panel narrows to them as its own search does (decision 0072).
import { inApp } from './app.svelte';
import { DOWNLOADS_LISTS } from './downloadsLists.svelte';
import { INSIGHTS_VIEWS } from './insightsViews';
import { me } from './me.svelte';
import { DOWNLOADS, HEALTH, INSIGHTS, REQUESTS, SETTINGS } from './panel.svelte';
import { problems } from './healthReport.svelte';
import { REQUESTS_LISTS } from './requests.svelte';
import { matchedOn, matches, provide, type SearchResult } from './search';
import { loadSettingsWords, settingsMatches } from './settingsSearch.svelte';
import { SECTIONS } from './settingsSections.svelte';

/** A page as the app lists it: its name and its address. */
export type PageLink = { label: string; href: string };

const at = (href: string, view: string) => `${href}?view=${encodeURIComponent(view)}`;
const result = (kind: string, subject: string, title: string, sub: string, query: string[]): SearchResult => ({
  kind,
  subject,
  title,
  sub,
  score: 1,
  matched: matchedOn(query, [{ on: 'name', text: title }]),
});

/** Registers the finders, for as long as the app runs; `pages()` gives the pages shown now (the mode's, the user's). */
export function provideEverywhere(pages: () => PageLink[]) {
  provide({
    kind: 'setting',
    label: 'Settings',
    find: (query) => {
      void loadSettingsWords();
      return SECTIONS.filter((x) => (!x.app || inApp) && (!x.admin || me.admin || x.everyone)).flatMap((x) => {
        const f = settingsMatches(x.id, x.title, query);
        if (!f.title && !f.rows.length) return [];
        return [
          {
            kind: 'setting',
            subject: at('#/settings', SETTINGS + x.id),
            title: x.title,
            sub: f.rows.length ? f.rows.slice(0, 3).join(' · ') : 'Settings',
            score: f.title ? 2 : 1,
            // Its line already names the rows found.
            matched: matchedOn(query, [{ on: 'name', text: x.title }]),
          },
        ];
      });
    },
  });
  // The control pages' parts, each page's under its name (the owner, 2026-10-25): Requests, Downloads, Insights, Health
  // and Settings. Home, Library and Discover only gather, so nothing of theirs; crates and playlists are sets, found as
  // the albums and artists are (the server's); a page itself is never a result.
  const PARTS: { href: string; label: string; parts: () => { view: string; label: string; sub?: string }[] }[] = [
    { href: '#/requests', label: 'Requests', parts: () => REQUESTS_LISTS.map((l) => ({ view: REQUESTS + l.id, label: l.label })) },
    { href: '#/downloads', label: 'Downloads', parts: () => DOWNLOADS_LISTS.map((l) => ({ view: DOWNLOADS + l.id, label: l.label })) },
    { href: '#/insights', label: 'Insights', parts: () => INSIGHTS_VIEWS.map((v) => ({ view: INSIGHTS + v.id, label: v.label, sub: v.group })) },
    // Its problems as far as its report is in (read when Health was opened).
    { href: '#/health', label: 'Health', parts: () => problems().map((x) => ({ view: HEALTH + x.key, label: x.title, sub: x.domain })) },
  ];
  for (const p of PARTS)
    provide({
      kind: `page:${p.href}`,
      label: p.label,
      find: (query) => {
        if (!pages().some((x) => x.href === p.href)) return [];
        return p.parts().map((x) => result(`page:${p.href}`, at(p.href, x.view), x.label, x.sub ?? '', query)).filter((r) => matches(query, r.title));
      },
    });
}
