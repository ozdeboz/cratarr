<script lang="ts" module>
  import type { IconName } from './Icon.svelte';
  import { port } from './player/port.svelte';

  /** A question the list asks once the menu has closed (lib/settings/ConfirmSheet): Delete's, Move's. */
  export type Ask = { title: string; text?: string | null; label: string; icon: IconName; go: () => void };
</script>

<script lang="ts">
  // Several ticked albums' own entries in the selection's menu (lib/SelectionMenu's `extra`; the owner, 2026-10-27:
  // what one has, several have): as an album's menu (lib/AlbumMenu) has them, each offered when it applies to one of
  // them at least and done to those it applies to. Start radio; the rating, all to the same stars; Sync to this device
  // and Remove from this device (in the app); what their libraries allow (Cancel conversion, Move to another, Delete;
  // administrators, decision 0034), Move and Delete asked once for them all, with one Undo.
  import { offerIn } from './AlbumMenu.svelte';
  import { albumOfDownload, albumRatings, library, loadAlbumRating, loadAlbumUses, rateAlbum, readLibrary, useOf } from './albums.svelte';
  import { cancelJob, deleteAlbum, moveAlbum, movePreview, type AlbumsSource, type KeepPreview, type LibraryInfo, type ListedAlbum, type ShelfSource } from './api';
  import { downloads, inApp, removeDownloads } from './app.svelte';
  import { DEVICE } from './deviceMusic.svelte';
  import { plural } from './insights.svelte';
  import { jobs } from './jobs.svelte';
  import { isKept, setKept } from './kept.svelte';
  import { startRadio, type LocalTrack } from './localPlayer.svelte';
  import { askIfMany, fewAtATime } from './many.svelte';
  import { me } from './me.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { albumGone } from './panel.svelte';
  import RatingStars from './RatingStars.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  type Ticked = { s: ShelfSource; a: ListedAlbum };
  let { list, tracks, close, onask }: { list: Ticked[]; tracks: LocalTrack[]; close: () => void; onask: (ask: Ask) => void } = $props();

  // The device's own albums have none of these (local mode's menu is its own).
  const items = $derived(list.filter((x) => x.s.id !== DEVICE));
  const n = $derived(plural(items.length, 'album'));
  /** What its library allows: the Library's list says (its sources carry it), else nothing. */
  const offers = $derived(items.map((x) => ({ ...x, o: 'deletable' in x.s ? offerIn(x.s as AlbumsSource, x.a) : null })));

  // Start radio (as one album's, on this device): tracks like all of theirs.
  function radio() {
    const [seed, name] = [tracks.map((t) => t.id), n];
    close();
    navigator.vibrate?.(8);
    startRadio(seed, name).catch((e) => toast.fail(e, 'Could not start playing'));
  }

  // The rating: the stars they share (none when they differ), a tap giving them all those, a tap on the shared one
  // taking it off. Only where their server keeps ratings, asked once per server (one album's rating says).
  const probe = $derived(new Map(items.map((x) => [x.s.id, x.a.id])));
  $effect(() => {
    for (const [source, id] of probe) {
      loadAlbumRating(source, id);
      loadAlbumUses(source);
    }
  });
  const rateable = $derived(items.filter((x) => albumRatings[`${x.s.id}/${probe.get(x.s.id)}`] != null));
  const ratingOf = (x: Ticked) => albumRatings[`${x.s.id}/${x.a.id}`] ?? useOf(x.s.id, x.a.id).rating;
  const shared = $derived(rateable.length && rateable.every((x) => ratingOf(x) === ratingOf(rateable[0])) ? ratingOf(rateable[0]) : 0);
  function rate(stars: number) {
    const to = shared === stars ? 0 : stars;
    // An album already at `to` is left alone (rateAlbum takes the rating it has off).
    const them = rateable.filter((x) => !to || albumRatings[`${x.s.id}/${x.a.id}`] !== to);
    const m = plural(them.length, 'album');
    askIfMany(them.length, { title: to ? `Give ${m} ${plural(to, 'star')}?` : `Take the rating off ${m}?`, label: to ? `Give ${plural(to, 'star')}` : 'Take it off', icon: 'star' }, () =>
      void fewAtATime(them, (x) => rateAlbum(x.s.id, x.a.id, to)),
    );
  }

  // Sync to this device (decision 0038): on for all, or off when all are.
  const allKept = $derived(items.length > 0 && items.every((x) => isKept('album', x.s.id, x.a.id)));
  function sync() {
    const on = !allKept;
    for (const x of items) if (isKept('album', x.s.id, x.a.id) !== on) setKept('album', x.s.id, x.a.id, on, x.a.title);
  }
  /** Their tracks kept on this device (the app's downloads), as one album's: Remove from this device. */
  const keys = $derived(new Set(items.map((x) => `${x.s.id}/${x.a.id}`)));
  const keptIds = $derived(inApp ? downloads.tracks.filter((t) => keys.has(`${t.source}/${albumOfDownload(t)}`)).map((t) => t.download.id) : []);

  // What their libraries allow (administrators): a job in the conversion queue only Cancel, else Move and Delete.
  const converting = $derived(me.admin ? offers.filter((x) => x.o?.job != null) : []);
  const deletable = $derived(me.admin ? offers.filter((x) => x.o && x.o.job === null && x.o.deletable) : []);
  /** Each library they can move into (by its name, whichever server's), with the albums that can. */
  const moves = $derived.by(() => {
    const by = new Map<string, (Ticked & { to: LibraryInfo; primary: boolean })[]>();
    if (me.admin)
      for (const x of offers)
        if (x.o && x.o.job === null) for (const t of x.o.targets) by.set(t.name, [...(by.get(t.name) ?? []), { s: x.s, a: x.a, to: t, primary: x.o.from?.use === 'music' }]);
    return [...by].map(([name, them]) => ({ name, them }));
  });

  /** Gone from the Library's list at once (as its page's `gone`), before it is read again. */
  function leave(gone: { source: string; id: string }[]) {
    const out = new Set(gone.map((x) => `${x.source}/${x.id}`));
    for (const x of gone) albumGone(x.source, x.id);
    const v = library.view;
    if (v) v.sources = v.sources.map((s) => ({ ...s, albums: s.albums.filter((a) => !out.has(`${s.id}/${a.id}`)) }));
  }
  const reread = () => readLibrary(true);
  /** "Moved 5 albums", or "Moved 4 of 5 albums" when some were refused. */
  const doneOf = (done: number, of: number) => (done === of ? plural(of, 'album') : `${done} of ${plural(of, 'album')}`);
  /** Names, the first few, then how many more. */
  const named = (titles: string[]) => (titles.length > 8 ? `${titles.slice(0, 8).join(', ')} and ${titles.length - 8} more` : titles.join(', '));

  function cancelAll() {
    const ids = converting.map((x) => x.o!.job!);
    close();
    fewAtATime(ids, cancelJob).then((failed) => {
      if (failed.length) toast.fail(null, failed.length === ids.length ? 'Could not cancel them' : "Not all were cancelled");
      jobs.poke();
    });
  }

  function askDelete() {
    const them = deletable.map((x) => ({ source: x.s.id, id: x.a.id, title: x.a.title, primary: x.o!.from?.use === 'music' }));
    const days = library.view?.trash_days ?? 14;
    const music = library.view?.music ?? 'Music';
    const inMusic = them.filter((x) => x.primary).length;
    const m = plural(them.length, 'album');
    close();
    // Asked once for them all; the primary library's extra step said in it (decision 0034).
    onask({
      title: `Delete ${m}?`,
      text: `They stay in the trash for ${plural(days, 'day')}.${inMusic ? ` ${inMusic === them.length ? (inMusic === 1 ? 'It is' : 'All are') : `${inMusic} are`} in ${music}, your main library.` : ''}`,
      label: `Delete ${m}`,
      icon: 'trash',
      go: async () => {
        const tokens: string[] = [];
        const gone: typeof them = [];
        // One at a time: each goes into its library's trash.
        await fewAtATime(
          them,
          async (x) => {
            tokens.push((await deleteAlbum(x.source, x.id)).undo);
            gone.push(x);
          },
          1,
        );
        leave(gone);
        if (!gone.length) return toast.fail(null, 'Could not delete the albums');
        withUndo(gone.length === 1 && them.length === 1 ? `Deleted ${gone[0].title}.` : `Deleted ${doneOf(gone.length, them.length)}.`, tokens, { after: reread });
      },
    });
  }

  // Move: what each would become asked first, then one question for them all (those refused named, left out).
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch these albums again.";
  async function askMove(name: string, them: (Ticked & { to: LibraryInfo; primary: boolean })[]) {
    // Read before closing: the menu (and this) is gone by the time the previews are in.
    const ask = onask;
    close();
    const seen: { x: (typeof them)[number]; p: KeepPreview }[] = [];
    const unread = await fewAtATime(them, async (x) => void seen.push({ x, p: await movePreview(x.s.id, x.a.id, x.to.id) }));
    const go = them.flatMap((x) => seen.filter((y) => y.x === x && !y.p.refused));
    const refused = seen.filter((y) => y.p.refused);
    if (!go.length) return toast.fail(null, `None of them can move to ${name}`);
    const convert = go.reduce((k, y) => k + y.p.convert, 0);
    const why = (p: KeepPreview) => (p.refused?.kind === 'needs_year' ? 'no year in the tags' : p.refused?.kind === 'other' ? p.refused.detail : '');
    const text = [
      `${named(go.map((y) => y.x.a.title))}.`,
      refused.length ? `Not moved: ${refused.slice(0, 8).map((y) => `${y.x.a.title} (${why(y.p)})`).join(', ')}${refused.length > 8 ? ` and ${refused.length - 8} more` : ''}.` : '',
      unread.length ? `${plural(unread.length, 'album')} couldn't be looked at.` : '',
      convert ? `${plural(convert, 'hi-res file')} ${convert === 1 ? 'is' : 'are'} converted to 16-bit/44.1 kHz first.` : '',
      go.some((y) => !y.p.acquisition && !y.x.primary) ? NO_LIDARR : '',
    ]
      .filter(Boolean)
      .join(' ');
    const m = plural(go.length, 'album');
    ask({
      title: `Move ${m} to ${name}?`,
      text,
      label: `Move to ${name}`,
      icon: 'intoBox',
      go: async () => {
        const tokens: (string | null)[] = [];
        const gone: { source: string; id: string }[] = [];
        let queued = false;
        await fewAtATime(
          go,
          async ({ x }) => {
            const r = await moveAlbum(x.s.id, x.a.id, x.to.id);
            // Queued (hi-res files to convert first): its row shows the progress instead.
            if (r.queued) queued = true;
            else (tokens.push(r.undo), gone.push({ source: x.s.id, id: x.a.id }));
          },
          1,
        );
        if (queued) jobs.poke();
        leave(gone);
        if (gone.length) withUndo(`Moved ${doneOf(gone.length, go.length)} to ${name}.`, tokens, { after: reread });
        else if (!queued) toast.fail(null, 'Could not move the albums');
      },
    });
  }
</script>

<MenuGroup>
  {#if port().radio && tracks.length && items.length}<MenuEntry icon="radio" label="Start radio" onclick={radio} />{/if}
</MenuGroup>
<!-- Feel: the rating (the heart is the select bar's). -->
<MenuGroup>
  {#if rateable.length}
    <div class="entry rating">
      <span>Rating</span>
      <RatingStars value={shared} onrate={rate} />
    </div>
  {/if}
</MenuGroup>
<MenuGroup>
  {#if inApp && items.length}
    <!-- Kept on this device (decision 0038), each of them. -->
    <MenuEntry icon="download" label="Sync to this device" switched={allKept} onclick={sync} />
  {/if}
  {#if keptIds.length}
    {@const ids = keptIds}
    <MenuEntry
      icon="trash"
      label="Remove from this device"
      danger
      onclick={() => {
        close();
        removeDownloads(ids);
      }}
    />
  {/if}
</MenuGroup>
<MenuGroup label="Library">
  {#if converting.length}<MenuEntry icon="close" label="Cancel conversion" onclick={cancelAll} />{/if}
  {#each moves as mv (mv.name)}<MenuEntry icon="intoBox" label={`Move to ${mv.name}`} onclick={() => askMove(mv.name, mv.them)} />{/each}
</MenuGroup>
<!-- Remove, last. -->
<MenuGroup>
  {#if deletable.length}<MenuEntry icon="trash" label="Delete albums" danger onclick={askDelete} />{/if}
</MenuGroup>

<style>
  /* The rating's card, as an album's menu has it: its word, then five stars. */
  .rating {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
    padding: 0 12px 0 20px;
    border-radius: 20px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    font-size: 16px;
  }

  .rating > span:first-child {
    flex: 1;
  }
</style>
