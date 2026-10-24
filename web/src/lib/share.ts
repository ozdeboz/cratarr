// Share (decided 2026-10-16, as Symfonium's): what it is in words and its link in Cratarr, through the phone's share
// sheet (the app's, in the app; the browser's where it has one), else copied. The link opens it in the panel for anyone
// who can reach this server.
import { inApp, toApp } from './app.svelte';
import { copyText } from './clipboard';
import { toast } from './toast.svelte';

/** The address that opens `subject` (a playlist's name, an album's subject) in the panel, over `page`. */
export const linkTo = (page: string, subject: string) => `${location.origin}/#/${page}?${new URLSearchParams({ view: subject })}`;

export async function share(title: string, text: string, url: string | null) {
  const words = url ? `${text}\n${url}` : text;
  if (inApp) return toApp({ type: 'share', title, text: words });
  if (navigator.share) {
    try {
      await navigator.share(url ? { title, text, url } : { title, text });
    } catch {
      /* closed without sharing */
    }
    return;
  }
  if (await copyText(words)) toast.show({ text: 'Copied', tone: 'done' });
  else toast.show({ text: "It couldn't be shared", tone: 'error' });
}
