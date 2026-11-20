<script lang="ts">
  // Upload music (UP04; UP01–UP03 are the server's): files picked, or a folder, checked whole against the room first (Q7: "Needs
  // 1.4 GB, 1 GB left"), then sent a piece at a time from where the server says it has each (lib/uploads), so a cut goes on from the
  // server's count and a reload lists the upload to pick its files again. A line each, its progress the server's; once placed, a
  // line each record, which opens it in Incoming. No word on success (minimal UI text): the lines say it. The pieces every sheet is
  // made of (MenuSheet, MenuEntry, Group, Item, Notice), and the browser's own file chooser.
  import { createUpload, uploadPiece, uploadStatus, type Upload } from './api';
  import { perAccount } from './account';
  import { floor } from './floor.svelte';
  import { me } from './me.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Notice from './Notice.svelte';
  import { albumSubject, viewOver } from './panel.svelte';
  import { pollEvery } from './pollEvery';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { lineProgress, matchPicked, pickedFolder, pickRefusal, sendFrom, wholeOf } from './uploads';

  let { onclose }: { onclose: () => void } = $props();

  // This device's uploads not yet placed, by id, so a reload can pick their files again (each read again from the server).
  const KEPT = 'cratarr-uploads';
  function keptIds(): string[] {
    try {
      const v = JSON.parse(localStorage.getItem(perAccount(KEPT)) ?? '[]');
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
    } catch {
      return [];
    }
  }
  function keep(ids: string[]) {
    try {
      localStorage.setItem(perAccount(KEPT), JSON.stringify(ids));
    } catch {
      /* not kept: a reload starts again */
    }
  }

  /** The upload under way, as the server last said it. */
  let upload = $state<Upload | null>(null);
  /** Each of its files' handle, in its files' order; none for one not picked again since a reload. */
  let handles = $state.raw<(File | null)[]>([]);
  let refusal = $state<string | null>(null);
  let failed = $state<string | null>(null);
  let sending = $state(false);
  /** Uploads of this device not finished, from before (a reload): their files to be picked again. */
  let unfinished = $state<Upload[]>([]);
  let resuming: Upload | null = null;
  let pickFiles = $state<HTMLInputElement>();
  let pickFolder = $state<HTMLInputElement>();

  const forget = (id: string) => keep(keptIds().filter((x) => x !== id));
  for (const id of keptIds())
    uploadStatus(id).then(
      (u) => {
        if (u.placed?.length) forget(id);
        else unfinished = [...unfinished, u];
      },
      () => forget(id),
    );

  /** The files chosen: a new upload (its whole size checked first), or the files of one from before matched again. */
  async function chosen(list: FileList | null) {
    const picked = [...(list ?? [])];
    if (!picked.length) return;
    refusal = null;
    const again = resuming;
    resuming = null;
    if (again) {
      unfinished = unfinished.filter((u) => u.id !== again.id);
      upload = again;
      handles = matchPicked(again.files, picked);
      return void send();
    }
    const room = me.room;
    refusal = pickRefusal(
      picked.reduce((n, f) => n + f.size, 0),
      room ? room.size - room.used : null,
      !!floor.value?.short,
    );
    if (refusal) return;
    try {
      upload = await createUpload(
        pickedFolder(picked),
        picked.map((f) => ({ name: f.name, size: f.size })),
      );
    } catch (e) {
      // The server's own words, as they are (its room, its floor, a name it refuses).
      refusal = e instanceof Error ? e.message : String(e);
      return;
    }
    keep([...keptIds(), upload.id]);
    handles = matchPicked(upload.files, picked);
    void send();
  }

  /** Each file sent from the server's count; a failure said with Try again, which goes on from the count read again. */
  async function send() {
    const u = upload;
    if (!u || sending) return;
    sending = true;
    failed = null;
    try {
      for (const [i, f] of u.files.entries()) {
        const file = handles[i];
        const now = upload?.files.find((x) => x.n === f.n) ?? f;
        if (!file || now.received >= now.size) continue;
        await sendFrom(file, now.received, async (offset, piece, digest) => {
          await uploadPiece(u.id, f.n, offset, piece, digest);
          upload = await uploadStatus(u.id);
          return upload.files.find((x) => x.n === f.n)?.received ?? offset + piece.size;
        });
      }
    } catch (e) {
      failed = e instanceof Error ? e.message : "The upload couldn't go on";
      upload = await uploadStatus(u.id).catch(() => upload);
    } finally {
      sending = false;
    }
  }

  // Every file whole: asked every 2 s until its records are placed (UP02: a pass reads them in).
  const placing = $derived(upload && !sending && !upload.placed?.length && upload.files.every((f) => f.received >= f.size) ? upload.id : null);
  $effect(() => {
    const id = placing;
    if (!id) return;
    return pollEvery(2_000, () => uploadStatus(id), (r) => {
      if (!r.ok) return;
      upload = r.value;
      if (r.value.placed?.length) forget(id);
    });
  });

  /** A file's line: how much of it the server has, or that its file is to be picked again. */
  const lineOf = (f: Upload['files'][number], i: number) =>
    f.received >= f.size ? 'Sent' : handles[i] ? `${Math.round(lineProgress(f) * 100)}%` : 'Choose it again to go on';

  function pickAgain(u: Upload) {
    resuming = u;
    pickFiles?.click();
  }
  function open(album: string) {
    onclose();
    viewOver(albumSubject('cratarr', album));
  }
</script>

<MenuSheet label="Upload music" icon="upload" title="Upload music" {onclose}>
  <input bind:this={pickFiles} type="file" multiple accept="audio/*,image/*" hidden onchange={(e) => void chosen(e.currentTarget.files)} />
  <input bind:this={pickFolder} type="file" webkitdirectory hidden onchange={(e) => void chosen(e.currentTarget.files)} />
  {#if !upload}
    {#if refusal}<p class="text" role="alert">{refusal}</p>{/if}
    {#if unfinished.length}
      <Group>
        {#each unfinished as u (u.id)}<Item name={u.folder ?? 'Upload'} line={wholeOf(u.files)} onclick={() => pickAgain(u)} />{/each}
      </Group>
    {/if}
    <MenuEntry icon="upload" label="Choose files" onclick={() => pickFiles?.click()} />
    <MenuEntry icon="manage" label="Choose a folder" onclick={() => pickFolder?.click()} />
  {:else}
    <div class="lines">
      {#if failed}<Notice text={failed} onretry={() => void send()} away="Uploading needs Cratarr." />{/if}
      <Group>
        {#if upload.placed?.length}
          {#each upload.placed as p (p.album)}<Item name={p.title} onclick={() => open(p.album)} />{/each}
        {:else}
          {#each upload.files as f, i (f.n)}<Item name={f.name} line={lineOf(f, i)} progress={f.received < f.size && handles[i] ? lineProgress(f) : null} />{/each}
        {/if}
      </Group>
    </div>
  {/if}
</MenuSheet>

<style>
  /* What refuses the pick, where a ConfirmSheet has its words. */
  .text {
    margin: 0;
    padding: 4px 24px 12px;
    color: var(--danger-ink);
  }

  .lines {
    padding: 0 16px 16px;
  }
</style>
