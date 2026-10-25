// What the global search finds in the browser itself (the owner, 2026-10-25: "Global search should also surface
// settings, and anything else from any other pages. It's a global search."): the pages, Settings' parts (by their
// names and their rows', lib/settingsSearch), Insights' views, the lists of Requests and Downloads. Each result's
// subject is the address that shows it, so a tap goes there (the panel opened on its part); the words searched stay,
// so the page or the panel narrows to them as its own search does (decision 0072).
import { inApp } from './app.svelte';
import { DOWNLOADS_LISTS } from './downloadsLists.svelte';
import { INSIGHTS_VIEWS } from './insightsViews';
import { me } from './me.svelte';
import { DOWNLOADS, INSIGHTS, REQUESTS, SETTINGS } from './panel.svelte';
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
  provide({
    kind: 'page',
    label: 'Pages',
    find: (query) => {
      const shown = pages();
      const has = (href: string) => shown.some((p) => p.href === href);
      return [
        ...shown.map((p) => result('page', p.href, p.label, 'Page', query)),
        ...(has('#/insights') ? INSIGHTS_VIEWS.map((v) => result('page', at('#/insights', INSIGHTS + v.id), v.label, `Insights · ${v.group}`, query)) : []),
        ...(has('#/requests') ? REQUESTS_LISTS.map((l) => result('page', at('#/requests', REQUESTS + l.id), l.label, 'Requests', query)) : []),
        ...(has('#/downloads') ? DOWNLOADS_LISTS.map((l) => result('page', at('#/downloads', DOWNLOADS + l.id), l.label, 'Downloads', query)) : []),
      ].filter((r) => matches(query, r.title));
    },
  });
}
