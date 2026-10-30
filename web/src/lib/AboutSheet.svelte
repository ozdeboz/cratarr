<script module lang="ts">
  /** Whether About shows (App renders it): opened from Settings' pill, its own button there. */
  export const about = $state({ open: false });
</script>

<script lang="ts">
  // About Cratarr (the owner, 2026-10-16): a sheet from the info button in the settings page's pill, in place of an
  // About card among the settings. To everyone (the owner, 2026-10-24): the version, since when it runs, the app's
  // version in the app, the licence and the source (the AGPL offers it to whoever uses Cratarr, 0029), and thanks to
  // what it builds on (README's Credits) and to the photographers of Home's skies (public/skies/CREDITS.txt). Settings
  // that can't be saved are a Health finding now.
  import { getAbout } from './api';
  import { appInfo, inApp } from './app.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { since } from './settings/editor.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let about = $state<{ version: string; started: number; licence?: string; source?: string } | null>(null);
  getAbout().then(
    (a) => (about = a),
    () => {},
  );

  const THANKS: [string, string][] = [
    ['Jellyfin', 'https://jellyfin.org'],
    ['Navidrome', 'https://www.navidrome.org'],
    ['gonic', 'https://github.com/sentriz/gonic'],
    ['nghe', 'https://github.com/vnghia/nghe'],
    ['Snapcast', 'https://github.com/badaix/snapcast'],
    ['Finamp', 'https://github.com/finamp-app/finamp'],
    ['Seerr', 'https://github.com/seerr-team/seerr'],
    ['AudioMuse-AI', 'https://github.com/NeptuneHub/AudioMuse-AI'],
    ['Beat This!', 'https://github.com/CPJKU/beat_this'],
    ['ListenBrainz', 'https://listenbrainz.org'],
  ];
  // Home's skies, from Unsplash.
  const PHOTOS: [string, string][] = [
    ['engin akyurt', 'lvmrwCLfDUg'],
    ['Nathan Anderson', 'L95xDkSSuWw'],
    ['Sam Schooler', 'E9aetBe2w40'],
    ['Ciocan Ciprian', 'i3SUvEFv0aA'],
    ['Barry Simon', '4C6Rp23RjnE'],
    ['Andriyko Podilnyk', 'E6ne6IUNxZw'],
    ['Patrick Hendry', '37ZuGYD3JOk'],
    ['Luca Bravo', 'cs6Omgsf0fU'],
    ['Joy Stamp', 'pGQbWXBC1dA'],
    ['Max van den Oetelaar', '1e60bqR1Ar0'],
    ['Daniel Lerman', 'GBkssTODNT0'],
    ['Raychel Sanner', '1cJXplTxrmI'],
    ['Chandler Cruttenden', 'w8hWTFpGtpY'],
    ['Dillon Kydd', 'ffIIyc3EN58'],
    ['Khamkéo', 'WtwSsqwYlA0'],
    ['Lena Tolmacheva', 'bNeo5CMDOJc'],
    ['James Day', '5YWf-5hyZcw'],
    ['Chris Linnett', 'hatFT0_uiEQ'],
    ['Mahdi Soheili', 'Ojo0zSaCphU'],
  ];
</script>

{#snippet links(list: [string, string][], href: (v: string) => string)}
  {#each list as [name, v], i (name)}<a href={href(v)} target="_blank" rel="noopener noreferrer">{name}</a>{i < list.length - 1 ? ', ' : ''}{/each}
{/snippet}

<MenuSheet label="About" title="Cratarr" icon="info" chips={about ? [about.version] : []} {onclose}>
  <dl>
    {#if about}
      <dt>Running since</dt>
      <dd>{since(about.started)}</dd>
    {/if}
    {#if inApp && appInfo.version}
      <dt>App</dt>
      <dd>{appInfo.version}</dd>
    {/if}
    {#if about?.licence}
      <dt>Licence</dt>
      <dd>{#if about.source}<a href={`${about.source}/blob/main/LICENSE`} target="_blank" rel="noopener noreferrer">{about.licence}</a>{:else}{about.licence}{/if}</dd>
    {/if}
    {#if about?.source}
      <dt>Source</dt>
      <dd><a href={about.source} target="_blank" rel="noopener noreferrer">{about.source.replace(/^https:\/\//, '')}</a></dd>
    {/if}
    <dt>Thanks to</dt>
    <dd>{@render links(THANKS, (v) => v)}</dd>
    <dt>Skies by</dt>
    <dd>{@render links(PHOTOS, (v) => `https://unsplash.com/photos/${v}`)} on Unsplash</dd>
  </dl>
</MenuSheet>

<style>
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 10px 16px;
    /* Padding, not margin: the sheet measures its content's height, which a child's margin would fall outside of. */
    margin: 0;
    padding: 4px 24px 20px;
    font-size: 15px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  a {
    color: inherit;
  }
</style>
