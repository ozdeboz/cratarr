// The settings as the Settings panel edits them (decided 2026-10-16: one small file per part, lib/settings/*Part): one
// editor for every part, so what is changed in one part shows the Save bar in all of them and nothing is lost going
// from one part to another. What it saves goes through the Save bar; Authentication's accounts and Import act at once.
import {
  ApiError,
  changeSubsonicPassword,
  checkNaming,
  createSubsonicAccount,
  getImport,
  getImportSources,
  getSettings,
  getSubsonicAccounts,
  previewLidarr,
  previewSource,
  saveSettings,
  startImport,
  type CatalogueStatus,
  type Choice,
  type Field,
  type ImportJob,
  type ImportSource,
  type ImportWhat,
  type Kind,
  type LibraryUse,
  type LidarrView,
  type NamingView,
  type Settings,
  type SourceConfig,
  type SourceView,
  type TagAlias,
  type TagRules,
} from '../api';
import { settingsRead, settingsSaved } from '../settingsSections.svelte';
import { toast } from '../toast.svelte';

/** A server (a source, in the code) as the page edits it. Secrets typed here are kept apart: the server never sends saved ones. */
export type Draft = SourceConfig & { secrets: Record<string, string>; view: SourceView | null; busy: boolean };
type ImportChoice = { user: string; account: string; what: ImportWhat };

const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x));
const fromWords = (text: string) => text.split(/\s+/).filter(Boolean);
/** Aliases as typed, a line each: "Hip-Hop → Hip Hop", "Electronica, Electronic Music → Electronic" (-> or = will do). */
const fromAliases = (text: string): TagAlias[] =>
  text
    .split('\n')
    .map((l) => l.split(/\s*(?:→|->|=>|=)\s*/))
    .filter((p) => p.length === 2 && !!p[0].trim() && !!p[1].trim())
    .map(([from, to]) => ({ from: from.split(',').map((f) => f.trim()).filter(Boolean), to: to.trim() }));
const toAliases = (list: TagAlias[] | undefined) => (list ?? []).map((a) => `${a.from.join(', ')} → ${a.to}`).join('\n');
const fromLines = (text: string) =>
  text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
export const str = (v: unknown) => (typeof v === 'string' ? v : '');
export const count = (n: number) => n.toLocaleString();
export const downloadedSize = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
export const since = (epoch: number) => new Date(epoch * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

export const USES: { id: LibraryUse; label: string }[] = [
  { id: 'music', label: 'Music' },
  { id: 'incoming', label: 'Incoming' },
  { id: 'unused', label: 'Not used' },
];
export const FORMATS = [
  { id: 'cd', label: 'CD format' },
  { id: 'as_is', label: 'As it is' },
] as const;
export const IMPORT_WHAT: { key: keyof ImportWhat; label: string }[] = [
  { key: 'plays', label: 'Plays' },
  { key: 'favorites', label: 'Favourites' },
  { key: 'ratings', label: 'Ratings' },
  { key: 'added', label: 'Added dates' },
];
const STEPS = { tracks: 'Tracks', groups: 'Favourite albums and artists', added: 'Added dates' };

class SettingsEditor {
  s = $state<Settings | null>(null);
  drafts = $state<Draft[]>([]);
  loadError = $state<string | null>(null);
  saving = $state(false);
  removing = $state<string | null>(null);
  addKind = $state('');

  // Saved with the servers: how tags are split. Edited as text (so Enter and spaces can be typed) and parsed when
  // compared or saved.
  ruleText = $state({ artistSeps: '', genreSeps: '', artistKeep: '', genreKeep: '', genreAliases: '', artistAliases: '' });
  cat = $state<CatalogueStatus | null>(null);
  trashDays = $state(14);

  // Music's naming template (decision 0013): the saved one, else the one the library follows, measured as it is typed.
  naming = $state('');
  namingInUse = $state('');
  namingCheck = $state<NamingView | null>(null);
  namingError = $state<string | null>(null);
  #namingTimer: ReturnType<typeof setTimeout> | undefined;

  // Subsonic apps' accounts (decision 0030): acted on at once, not through the Save bar. `known`: asked once (the
  // accounts null after it: they could not be read), so Import says nothing until then (the owner, 2026-10-24).
  sub = $state({ accounts: null as string[] | null, known: false, user: '', current: '', password: '', busy: false });

  // Importing plays and favourites from a server into an account: the choices per server, and the import running now
  // or the last one (one at a time), followed while it runs. `known`: the servers asked once (until then no "No server").
  imp = $state({ sources: [] as ImportSource[], accounts: [] as string[], choice: {} as Record<string, ImportChoice>, job: null as ImportJob | null, known: false });
  importing = $derived(!!this.imp.job && this.imp.job.finished === null);

  // Lidarr as the page edits it; the saved key is never sent here.
  lid = $state({
    url: '',
    key: '',
    enabled: true,
    // What Discover's Want adds an album with (null: as the artists Lidarr has).
    quality: null as number | null,
    metadata: null as number | null,
    root: null as string | null,
    view: null as LidarrView | null,
    busy: false,
    removing: false,
  });

  rules = $derived<TagRules>({
    artist_separators: fromWords(this.ruleText.artistSeps),
    genre_separators: fromWords(this.ruleText.genreSeps),
    artist_keep: fromLines(this.ruleText.artistKeep),
    genre_keep: fromLines(this.ruleText.genreKeep),
    genre_aliases: fromAliases(this.ruleText.genreAliases),
    artist_aliases: fromAliases(this.ruleText.artistAliases),
  });
  lidarrChanged = $derived(
    !!this.s &&
      (this.lid.url.trim().replace(/\/+$/, '') !== (this.s.lidarr?.url ?? '') ||
        this.lid.key.trim() !== '' ||
        (!!this.s.lidarr &&
          (this.lid.enabled !== this.s.lidarr.enabled ||
            this.lid.quality !== this.s.lidarr.quality_profile ||
            this.lid.metadata !== this.s.lidarr.metadata_profile ||
            this.lid.root !== this.s.lidarr.root_folder))),
  );
  /** The saved key is only reused for the same address. */
  lidarrKeyNeeded = $derived(!!this.s?.lidarr?.key_set && this.lid.url.trim().replace(/\/+$/, '') !== this.s.lidarr.url && this.lid.url.trim() !== '');
  /** Lidarr only matters to Incoming. */
  showLidarr = $derived(!!this.s?.lidarr || this.drafts.some((d) => d.enabled && d.libraries.incoming.length > 0));
  /** The colours to choose from (none from a server before it sent them: only the system's picker then). */
  palette = $derived(this.s?.palette ?? []);

  dirty = $derived(
    !!this.s &&
      (this.drafts.some((d) => Object.values(d.secrets).some((v) => v.trim())) ||
        this.lidarrChanged ||
        JSON.stringify(this.rules) !== JSON.stringify(this.s.tag_rules) ||
        this.trashDays !== this.s.trash_days ||
        (!!this.namingInUse && this.naming.trim() !== this.namingInUse) ||
        JSON.stringify(this.drafts.map((d) => this.payload(d))) !==
          JSON.stringify(this.s.sources.map((v) => this.payload({ ...v, secrets: {}, view: v, busy: false })))),
  );

  /** Reads everything: the last answer at once (the cards' read), then the server's latest, unless something was
   *  edited meanwhile; the accounts and the imports. */
  load = () => {
    if (settingsRead.s && !this.s) this.fill(settingsRead.s);
    getSettings()
      .then((v) => {
        settingsRead.s = v;
        if (!this.s || !this.dirty) this.fill(v);
      })
      .catch((e) => (this.loadError = e instanceof ApiError ? e.message : 'Unexpected error'));
    getSubsonicAccounts()
      .then((a) => (this.sub.accounts = a.accounts))
      .catch(() => (this.sub.accounts = null))
      .finally(() => (this.sub.known = true));
    this.loadImports();
  };

  fill = (next: Settings) => {
    this.s = next;
    this.ruleText = {
      artistSeps: next.tag_rules.artist_separators.join(' '),
      genreSeps: next.tag_rules.genre_separators.join(' '),
      artistKeep: next.tag_rules.artist_keep.join('\n'),
      genreKeep: next.tag_rules.genre_keep.join('\n'),
      genreAliases: toAliases(next.tag_rules.genre_aliases),
      artistAliases: toAliases(next.tag_rules.artist_aliases),
    };
    this.cat = next.catalogue;
    this.lid = {
      url: next.lidarr?.url ?? '',
      key: '',
      enabled: next.lidarr?.enabled ?? true,
      quality: next.lidarr?.quality_profile ?? null,
      metadata: next.lidarr?.metadata_profile ?? null,
      root: next.lidarr?.root_folder ?? null,
      view: next.lidarr,
      busy: false,
      removing: false,
    };
    this.trashDays = next.trash_days;
    if (next.music_folder) this.loadNaming();
    this.drafts = next.sources.map((v) => ({
      id: v.id,
      kind: v.kind,
      enabled: v.enabled,
      libraries: clone(v.libraries),
      values: clone(v.values),
      secrets: {},
      view: v,
      busy: false,
    }));
    this.addKind = next.kinds[0]?.id ?? '';
  };

  loadNaming = async () => {
    try {
      const v = await checkNaming();
      this.namingInUse = v.template;
      this.naming = v.template;
      this.namingCheck = v;
      this.namingError = null;
    } catch {
      /* no Music yet: the field stays hidden */
    }
  };

  typedNaming = () => {
    clearTimeout(this.#namingTimer);
    this.#namingTimer = setTimeout(async () => {
      const typed = this.naming;
      try {
        const v = await checkNaming(typed);
        if (typed !== this.naming) return;
        this.namingCheck = v;
        this.namingError = null;
      } catch (e) {
        if (typed !== this.naming) return;
        this.namingCheck = null;
        this.namingError = e instanceof ApiError ? e.message : 'Unexpected error';
      }
    }, 300);
  };

  createAccount = async () => {
    this.sub.busy = true;
    try {
      this.sub.accounts = (await createSubsonicAccount(this.sub.user.trim(), this.sub.password)).accounts;
      this.sub.password = '';
      this.loadImports();
    } catch (e) {
      toast.fail(e, 'Could not create the account');
    } finally {
      this.sub.busy = false;
    }
  };

  changePassword = async () => {
    if (!this.sub.accounts?.length) return;
    this.sub.busy = true;
    try {
      await changeSubsonicPassword(this.sub.accounts[0], this.sub.current, this.sub.password);
      this.sub.current = '';
      this.sub.password = '';
    } catch (e) {
      toast.fail(e, 'Could not change the password');
    } finally {
      this.sub.busy = false;
    }
  };

  loadImports = async () => {
    try {
      const [srcs, last] = await Promise.all([getImportSources(), getImport()]);
      this.imp.accounts = srcs.accounts;
      for (const src of srcs.sources) {
        // Kept while the page is open, unless it had no account to go into yet.
        if (this.imp.choice[src.id]?.account) continue;
        this.imp.choice[src.id] = {
          user: src.user ?? src.users[0]?.id ?? '',
          account: src.account ?? srcs.accounts[0] ?? '',
          what: { plays: true, favorites: true, ratings: true, added: true, ...src.what },
        };
      }
      this.imp.sources = srcs.sources;
      this.imp.job = last.job;
    } catch {
      /* no servers to import from: the part says so */
    }
    this.imp.known = true;
  };

  runImport = async (source: string, preview: boolean, password?: string) => {
    const c = this.imp.choice[source];
    try {
      this.imp.job = (await startImport({ source, user: c.user, account: c.account, what: { ...c.what }, preview, password })).job;
    } catch (e) {
      toast.fail(e, 'Could not start the import');
    }
  };

  /** While an import runs, its progress (Import's effect calls it). */
  followImport = async () => {
    try {
      this.imp.job = (await getImport()).job;
    } catch {
      /* the last known progress stays */
    }
  };

  importLine = (j: ImportJob) => {
    const c = j.counts;
    const parts = [
      c.plays && `${count(c.plays)} plays on ${count(c.songs)} ${c.songs === 1 ? 'song' : 'songs'}`,
      c.starred && `${count(c.starred)} ${c.starred === 1 ? 'favourite' : 'favourites'}`,
      c.unstarred && `${count(c.unstarred)} no longer ${c.unstarred === 1 ? 'a favourite' : 'favourites'}`,
      c.rated && `${count(c.rated)} ${c.rated === 1 ? 'rating' : 'ratings'}`,
      c.added && `${count(c.added)} added ${c.added === 1 ? 'date' : 'dates'}`,
      c.skipped && `${count(c.skipped)} not in the library`,
    ].filter(Boolean);
    const line = parts.length ? parts.join(' · ') : 'Nothing new';
    return j.preview ? `Preview: ${line}` : line;
  };

  stepLine = (j: ImportJob) => {
    const st = j.step;
    if (!st) return { label: 'Starting', value: 0 };
    const of = st.total ? ` · ${count(st.read)} of ${count(st.total)}` : st.read ? ` · ${count(st.read)}` : '';
    return { label: `${STEPS[st.name]}${of}`, value: st.total ? st.read / st.total : 0 };
  };

  kindOf = (d: { kind: string }): Kind | undefined => this.s?.kinds.find((k) => k.id === d.kind);
  saved = (d: Draft) => this.s?.sources.find((v) => v.id === d.id && v.kind === d.kind);

  /** What is sent: blank secrets are left out, so the saved ones are kept (where the server allows). */
  payload = (d: Draft): SourceConfig => {
    const values = { ...d.values };
    for (const [k, v] of Object.entries(d.secrets)) if (v.trim()) values[k] = v.trim();
    return { id: d.id, kind: d.kind, enabled: d.enabled, libraries: clone(d.libraries), values };
  };

  useOf = (d: Draft, id: string): LibraryUse => (d.libraries.music.includes(id) ? 'music' : d.libraries.incoming.includes(id) ? 'incoming' : 'unused');

  setUse = (d: Draft, id: string, use: LibraryUse) => {
    d.libraries.music = d.libraries.music.filter((x) => x !== id);
    d.libraries.incoming = d.libraries.incoming.filter((x) => x !== id);
    if (use !== 'unused') d.libraries[use].push(id);
  };

  // A used library's own settings (decision 0034): what a move into it does to the files (CD format converts hi-res
  // to 16-bit/44.1 kHz; as it is moves them unchanged), and for one other than the primary its dot's colour. Shown as
  // they apply (the server's), changed here with the rest and saved with Save.
  policyOf = (d: Draft, id: string) => ({
    format: 'as_is' as 'cd' | 'as_is',
    colour: null as string | null,
    ...d.view?.policies?.[id],
    ...d.libraries.policies?.[id],
  });
  setPolicy = (d: Draft, id: string, change: { format?: 'cd' | 'as_is'; colour?: string }) => {
    d.libraries.policies = { ...(d.libraries.policies ?? {}), [id]: { ...this.policyOf(d, id), ...change } };
  };

  checkLidarr = async () => {
    this.lid.busy = true;
    try {
      this.lid.view = await previewLidarr({ url: this.lid.url, api_key: this.lid.key });
      // The answer to a check is a toast, either way.
      if (this.lid.view?.error) toast.show({ text: `Lidarr: ${this.lid.view.error}`, tone: 'error' });
      else if (this.lid.view) toast.show({ text: `Connected to Lidarr${this.lid.view.version ? ` ${this.lid.view.version}` : ''}.` });
    } catch (e) {
      toast.fail(e, 'Could not check Lidarr');
    } finally {
      this.lid.busy = false;
    }
  };

  /** A secret must be typed again when the source is new, has none saved, or a field it is bound to changed. */
  secretNeeded = (d: Draft, f: Field) => {
    const before = this.saved(d);
    if (!before || !before.secrets_set.includes(f.key)) return f.required;
    return f.bound_to.some((b) => str(d.values[b]) !== str(before.values[b]));
  };

  /** The choices for a field, always including the current value so nothing is silently dropped. */
  choicesFor = (d: Draft, f: Field): Choice[] => {
    const list = d.view?.choices[f.key] ?? [];
    const cur = str(d.values[f.key]);
    return cur && !list.some((c) => c.value === cur) ? [{ value: cur, label: cur }, ...list] : list;
  };

  check = async (d: Draft) => {
    d.busy = true;
    try {
      d.view = await previewSource(this.payload(d));
      // The answer to a check is a toast, either way.
      const st = d.view.status;
      if (st.connected) {
        const name = [st.server_name ?? this.kindOf(d)?.label ?? d.kind, st.version].filter(Boolean).join(' ');
        toast.show({ text: `Connected to ${name}.` });
      } else {
        toast.show({ text: st.error ?? d.view.error ?? 'Not connected', tone: 'error' });
      }
    } catch (e) {
      toast.fail(e, 'Could not check the connection');
    } finally {
      d.busy = false;
    }
  };

  addServer = (kind: string) => {
    const k = this.s?.kinds.find((x) => x.id === kind);
    if (!k) return;
    let id = k.id;
    for (let n = 2; this.drafts.some((d) => d.id === id); n++) id = `${k.id}-${n}`;
    this.drafts.push({ id, kind: k.id, enabled: true, libraries: { music: [], incoming: [] }, values: {}, secrets: {}, view: null, busy: false });
  };

  save = async () => {
    this.saving = true;
    try {
      const lidarr = this.lidarrChanged
        ? {
            url: this.lid.url.trim(),
            api_key: this.lid.key.trim(),
            enabled: this.lid.enabled,
            quality_profile: this.lid.quality,
            metadata_profile: this.lid.metadata,
            root_folder: this.lid.root,
          }
        : undefined;
      const namingChanged = !!this.namingInUse && this.naming.trim() !== this.namingInUse;
      this.fill(
        await saveSettings(
          this.drafts.map((d) => this.payload(d)),
          { tag_rules: this.rules, lidarr, trash_days: this.trashDays, naming: namingChanged ? this.naming.trim() : undefined },
        ),
      );
      settingsRead.s = this.s;
      settingsSaved.n++;
      toast.show({ text: 'Saved and in use.' });
    } catch (e) {
      toast.fail(e, 'Could not save');
    } finally {
      this.saving = false;
    }
  };

  statusLine = (v: SourceView | null) => {
    if (!v) return null;
    const st = v.status;
    if (st.connected) return { ok: true, text: ['Connected to', st.server_name ?? v.label, st.version, st.detail].filter(Boolean).join(' ') };
    return { ok: false, text: st.error ?? v.error ?? 'Not connected' };
  };

  serverName = (id: string) => {
    // A Subsonic server imported from (decision 0067): its address, without the scheme.
    if (id.startsWith('subsonic:')) return id.slice('subsonic:'.length).replace(/^https?:\/\//, '');
    const v = this.s?.sources.find((x) => x.id === id);
    return v ? (this.s!.sources.length > 1 ? `${v.label} (${id})` : v.label) : id;
  };

  catalogueLine = (c: CatalogueStatus) => {
    const read = c.files - c.tags_pending;
    const parts = [`${count(c.files)} tracks`];
    if (c.tags_pending > 0) parts.push(`tags read for ${count(read)} (${Math.floor((100 * read) / Math.max(c.files, 1))}%)`);
    for (const [id, n] of Object.entries(c.linked)) parts.push(`${count(n)} known to ${this.serverName(id)}`);
    const problems = Object.values(c.issues).reduce((a, b) => a + (b ?? 0), 0);
    if (problems > 0) parts.push(`${count(problems)} with problems (see Repairs)`);
    const when = c.progress.last_pass ? new Date(c.progress.last_pass * 1000).toLocaleTimeString(undefined, { timeStyle: 'short' }) : null;
    return { parts, doing: c.progress.doing, when };
  };

  /** The labels of the given fields, for "you changed the server address". */
  labelOf = (d: Draft, keys: string[]) => keys.map((k) => this.kindOf(d)?.fields.find((f) => f.key === k)?.label.toLowerCase() ?? k).join(' and ');
}

/** The one editor: every part of the Settings panel edits it. */
export const ed = new SettingsEditor();
