// What playing in the background needs, asked once as the app starts (the owner, 2026-11-07: "on startup to do a permission check
// and ask to set the permissions as a shortcut to the existing controls"; the tablet that night: killed for memory while it
// played, it was refused the foreground on its restart until opened, and with the exemption it played on within 7 s). A shortcut
// to This device's Background row, no setting of its own: answered either way it is not asked again, and that row stays where
// it is taken back or given later. Notifications are the app's own ask (MainActivity), as Android's dialog needs no page.
import { allowBackground, appInfo, inApp } from './app.svelte';
import { prefs } from './prefs.svelte';

export const backgroundAsk = {
  /** The app has said how it stands, Android may stop it, and this device was not asked yet. */
  get due() {
    return inApp && appInfo.version !== null && !appInfo.batteryAllowed && !prefs.backgroundAsked;
  },
  /** Yes: Android's own dialog, as the Background row's switch opens it. */
  allow: allowBackground,
  /** Asked: answered, or the sheet closed. */
  asked() {
    prefs.backgroundAsked = true;
  },
};
