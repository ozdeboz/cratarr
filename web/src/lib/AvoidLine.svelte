<script lang="ts">
  // A ranked crate's "Avoiding …" line (the owner, 2026-10-23: playlist fit models, every ranked crate's): what it has
  // learned not to suggest, from what was turned down for it, and how many rules of one's own; a tap opens its Not for
  // sheet (lib/NotForSheet) to dismiss a trait, write a rule or put a track back. `round`: read again when it changes
  // (something turned down or put back).
  import { crateLeftOut, getCrateFit, leaveOutOfCrate, trackState, traitLabel, type CrateFit } from './api';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';
  import Icon from './Icon.svelte';
  import NotForSheet from './NotForSheet.svelte';

  /** `seed`: a Sounds like crate's seed (an album's al-… or a track's tr-…, Cratarr's own): Hide album beside the line
   *  leaves the seed's album out of the crate (the owner, 2026-10-25), as a crate's albums are left out by hand
   *  (decision 0024, amended: its tracks now and later; the one store, the owner). Once, not a toggle: it goes then,
   *  the line counts the albums left out, and the Not for sheet lists them to put back. */
  let { crate, name, round = 0, seed = null, onchange = () => {} }: { crate: string; name: string; round?: number; seed?: string | null; onchange?: () => void } = $props();

  /** The seed's album: an album seed is it, a track's says. */
  let seedAlbum = $state<string | null>(null);
  $effect(() => {
    const s = seed;
    seedAlbum = s?.startsWith('al-') ? s : null;
    if (s?.startsWith('tr-')) trackState('cratarr', s).then((t) => s === seed && (seedAlbum = t.album_id), () => {});
  });
  /** The crate's albums left out, as last read. */
  let outAlbums = $state<string[]>([]);
  $effect(() => {
    const c = crate;
    void [round, saved];
    crateLeftOut(c).then(
      (l) => c === crate && (outAlbums = (l.albums ?? []).map((a) => a.id)),
      () => {},
    );
  });
  const hidden = $derived(!!seedAlbum && outAlbums.includes(seedAlbum));
  let hiding = $state(false);
  async function hideAlbum() {
    const a = seedAlbum;
    if (!a || hiding || hidden) return;
    hiding = true;
    try {
      const { undo } = await leaveOutOfCrate(crate, 'cratarr', { albums: [a] });
      outAlbums = [...outAlbums, a];
      withUndo(`The album is left out of ${name}.`, [undo], { after: () => (saved++, onchange()) });
      saved++;
      onchange();
    } catch (e) {
      toast.fail(e, "The album wasn't left out");
    } finally {
      hiding = false;
    }
  }

  let fit = $state<CrateFit | null>(null);
  let open = $state(false);
  let saved = $state(0);
  /** The crate `fit` is of: another one's line goes at once, not shown while this one's comes; a read again keeps it
   *  (the owner, 2026-10-24). */
  let fitOf = '';
  $effect(() => {
    const c = crate;
    void [round, saved];
    if (c !== fitOf) {
      fitOf = c;
      fit = null;
    }
    getCrateFit(c).then(
      (f) => {
        if (c === crate) fit = f;
      },
      () => (fit = null),
    );
  });
  const avoiding = $derived((fit?.learned ?? []).filter((l) => !l.dismissed).map((l) => traitLabel(l.trait)));
  const rules = $derived(fit?.rules.length ?? 0);
  /** After what it learned: its albums left out and its rules; nothing at all, "Nothing avoided yet". */
  const also = $derived(
    [outAlbums.length ? `${outAlbums.length} ${outAlbums.length === 1 ? 'album' : 'albums'} left out` : null, rules ? `${rules} ${rules === 1 ? 'rule' : 'rules'}` : null].filter(
      (x): x is string => !!x,
    ),
  );
</script>

{#if fit}
  <div class="line">
    <button class="avoid" onclick={() => (open = true)}>
      <Icon name="sliders" size={16} />
      <span
        >{#if avoiding.length}Avoiding {#each avoiding as a, i (a)}{#if i}{' · '}{/if}<b>{a}</b>{/each}{/if}{#each also as x, i (x)}{avoiding.length || i ? ' · ' : ''}{x}{/each}{#if !avoiding.length && !also.length}Nothing avoided yet{/if}</span
      >
    </button>
    {#if seedAlbum && !hidden}
      <button class="avoid pill" disabled={hiding} aria-busy={hiding || undefined} onclick={hideAlbum}><Icon name="eyeOff" size={16} /><span>Hide album</span></button>
    {/if}
  </div>
{/if}
{#if open}
  <NotForSheet
    {crate}
    {name}
    onchange={() => {
      saved++;
      onchange();
    }}
    onclose={() => (open = false)}
  />
{/if}

<style>
  /* The line and Hide album beside it, in the line's place. */
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 4px 16px 6px;
  }

  .line .avoid {
    margin: 0;
  }

  /* A pill: round-ended, as wide as its words. */
  .avoid.pill {
    flex: none;
    border-radius: 999px;
  }

  .avoid {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 32px);
    margin: 4px 16px 6px;
    padding: 8px 12px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--muted);
    font: inherit;
    font-size: 0.88rem;
    text-align: start;
    cursor: pointer;
  }

  .avoid :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .avoid span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  b {
    color: var(--text);
    font-weight: 500;
  }
</style>
