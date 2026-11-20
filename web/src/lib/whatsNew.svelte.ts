// What's New (UC03; the rules lib/whatsNewRule, UC02 the server's): read when Settings or About shows, its dot on Settings' info
// pill and in About; opened, its sheet (lib/WhatsNewSheet, App's), and the newest it shows marked seen, so the dot goes.
import { getWhatsNew, whatsNewSeen } from './api';
import { newestOf, whatsNewDot, type WhatsNew } from './whatsNewRule';

export const whatsNew = $state({ value: null as WhatsNew | null, open: false });

let loading: Promise<void> | null = null;
/** Reads what this account has not seen; not read, there is nothing to show, and it is asked again next time. */
export function loadWhatsNew(): Promise<void> {
  loading ??= getWhatsNew()
    .then(
      (w) => void (whatsNew.value = w),
      () => {},
    )
    .finally(() => (loading = null));
  return loading;
}

/** Whether the dot shows. */
export const whatsNewShows = () => whatsNewDot(whatsNew.value);

/** Its sheet opened, and the newest it shows marked seen: the dot goes at once, whatever the answer. */
export function openWhatsNew() {
  whatsNew.open = true;
  const w = whatsNew.value;
  const newest = newestOf(w);
  if (!w || !w.unread || !newest) return;
  whatsNew.value = { ...w, unread: false };
  void whatsNewSeen(newest).catch(() => {});
}
