<script lang="ts">
  // Pair an app (QR04): Account's own page for it, the QR and the code and nothing else (QR02: the QR the server draws, of the
  // page's own address and the code). Asked every 2 s what became of the code, on a timer (lib/pollEvery); once paired, or the code
  // gone, back to the list, where the new sign-in shows. Opened with no code (by its address): back at once.
  import { untrack } from 'svelte';
  import { PAIR_QR, pairState } from '../api';
  import { loadMe } from '../me.svelte';
  import { backFromThing } from '../panel.svelte';
  import { pairStep } from '../pairing';
  import { pairs } from '../pairs.svelte';
  import { pollEvery } from '../pollEvery';
  import { nameThing } from '../settingsSections.svelte';

  nameThing(() => 'Pair an app');
  $effect(() => {
    if (!untrack(() => pairs.code)) return void backFromThing('account');
    return pollEvery(2_000, pairState, (r) => {
      if (!r.ok || pairStep(r.value) === 'show') return;
      pairs.code = null;
      void loadMe();
      backFromThing('account');
    });
  });
</script>

{#if pairs.code}
  <div class="pair">
    <img class="qr" src={`${PAIR_QR}?code=${encodeURIComponent(pairs.code)}`} alt={`QR code for ${pairs.code}`} />
    <p class="code">{pairs.code}</p>
  </div>
{/if}

<style>
  .pair {
    display: grid;
    justify-items: center;
    gap: 16px;
    padding: 24px 0;
  }

  /* On white whatever the theme, as a QR reads best (the SVG's own quiet zone around it). */
  .qr {
    width: min(280px, 100%);
    aspect-ratio: 1;
    border-radius: 14px;
    background: #fff;
    image-rendering: pixelated;
  }

  .code {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 28px;
    font-weight: 600;
    letter-spacing: 0.2em;
  }
</style>
