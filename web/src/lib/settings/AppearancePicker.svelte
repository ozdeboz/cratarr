<script lang="ts">
  // Light or dark, as Apple's Display & Brightness (the owner, 2026-10-25, mockup T3): two small pictures of the app in
  // each look, a round tick under the one shown; Automatic (Display's next row) follows the system instead. A picture
  // tapped while Automatic is on takes that look and switches Automatic off. Its colours are the themes' own (app.css),
  // so each picture shows its look whatever the app's is now.
  import { matchesSearch } from '../pageView.svelte';
  import { prefs } from '../prefs.svelte';
  import { settingsNarrowed } from './narrow';

  const LOOKS: { value: 'light' | 'dark'; label: string }[] = [
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];

  // The system's look, followed while Automatic is on.
  const media = typeof matchMedia !== 'undefined' ? matchMedia('(prefers-color-scheme: dark)') : null;
  let systemDark = $state(media?.matches ?? true);
  $effect(() => {
    if (!media) return;
    const on = () => (systemDark = media.matches);
    media.addEventListener('change', on);
    return () => media.removeEventListener('change', on);
  });
  /** The look shown now: the chosen one, or the system's while Automatic is on. */
  const shownLook = () => (prefs.theme === 'auto' ? (systemDark ? 'dark' : 'light') : prefs.theme);

  // Left out by the panel's own search on a phone unless it asks for this (lib/settings/narrow).
  const narrowed = settingsNarrowed();
  const left = $derived(narrowed() && !matchesSearch('Theme', 'Appearance', 'Light', 'Dark'));
</script>

<div class="looks" class:left role="radiogroup" aria-label="Theme">
  {#each LOOKS as l (l.value)}
    {@const on = shownLook() === l.value}
    <button type="button" class="look" class:on role="radio" aria-checked={on} onclick={() => (prefs.theme = l.value)}>
      <span class="picture {l.value}" aria-hidden="true"><i class="accent"></i><i></i><i></i><i class="short"></i><i></i></span>
      <span class="name">{l.label}</span>
      <span class="tick" aria-hidden="true"></span>
    </button>
  {/each}
</div>

<style>
  .looks {
    display: flex;
    justify-content: center;
    gap: 28px;
    padding: 16px 12px 12px;
  }

  .looks.left {
    display: none;
  }

  .look {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  /* The app in that look: its page, a line in the accent and a few of text. */
  .picture {
    display: grid;
    align-content: start;
    gap: 6px;
    box-sizing: border-box;
    width: 74px;
    height: 128px;
    padding: 10px 8px;
    border: 2px solid var(--border);
    border-radius: 14px;
  }

  .picture.light {
    background: #f5f6f8;
  }

  .picture.dark {
    background: #0f1115;
  }

  .picture i {
    display: block;
    height: 7px;
    border-radius: 4px;
  }

  .picture.light i {
    background: #d9dde5;
  }

  .picture.dark i {
    background: #2a2f3a;
  }

  .picture i.accent {
    width: 60%;
    background: var(--accent);
  }

  .picture i.short {
    width: 70%;
  }

  .look.on .picture {
    border-color: var(--accent);
  }

  .look:focus-visible .picture {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .name {
    font-size: 14px;
  }

  /* A round mark, ticked under the look shown (ChoiceList's marks, with Apple's tick). */
  .tick {
    display: grid;
    place-items: center;
    box-sizing: border-box;
    width: 22px;
    height: 22px;
    border: 2px solid var(--border);
    border-radius: 50%;
  }

  .look.on .tick {
    border-color: var(--accent);
    background: var(--accent);
  }

  .look.on .tick::after {
    content: '';
    width: 5px;
    height: 10px;
    border: solid var(--accent-text);
    border-width: 0 2px 2px 0;
    transform: translateY(-1px) rotate(45deg);
  }
</style>
