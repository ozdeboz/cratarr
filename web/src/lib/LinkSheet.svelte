<script lang="ts">
  // A link that leaves Cratarr (the owner, 2026-10-24: "prompted using the normal way to open the link or copy the
  // link"): its address, Open and Copy link, in the app's usual sheet. Every such link comes here (App: a tap on a link
  // to another site), so they all ask alike; Open goes to the browser (the app's own window for another site).
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { copyText } from './clipboard';
  import { toast } from './toast.svelte';

  let { url, name, onclose }: { url: string; name: string; onclose: () => void } = $props();

  function open() {
    window.open(url, '_blank', 'noopener');
    onclose();
  }
  async function copy() {
    const copied = await copyText(url);
    onclose();
    if (!copied) toast.show({ text: "It couldn't be copied", tone: 'error' });
  }
</script>

<MenuSheet label="Link" icon="link" title={name} chips={[url.replace(/^https?:\/\//, '').replace(/\/$/, '')]} {onclose}>
  <MenuEntry icon="externalLink" label="Open" onclick={open} />
  <MenuEntry icon="copy" label="Copy link" onclick={copy} />
</MenuSheet>
