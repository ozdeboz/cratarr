// Basic and Advanced in Settings (CU22's page side, CU23; the owner, 2026-11-06: "one global toggle for the control panels";
// Q5, Q6): one switch per account, kept on it (`settings_advanced`, lib/accountPrefs), an icon in the glass that lights while on
// Advanced. Basic only hides: Personal's and This device's rows that are not Basic (lib/settings/tiers), and the parts left with
// none. A part opened by its address shows whole: only one reached from Settings' own list, or its pickers, is narrowed.
import { prefs } from '../prefs.svelte';
import type { SettingsSection } from '../settingsSections.svelte';

/** Basic on: the account said so. None held yet is Advanced: nothing hides before the account says (Q5). */
export const settingsBasic = () => prefs.settingsAdvanced === false;

/** The switch in the glass. */
export function toggleAdvanced() {
  prefs.settingsAdvanced = settingsBasic();
}

/** The part last reached from Settings' list or a part's pickers (lib/Settings, lib/SettingsPanel, App's part beside the list):
 *  only that one is narrowed by Basic; any other way there (a link, the address, Back from elsewhere) shows a part whole. */
export const settingsListed = $state({ part: null as SettingsSection | null });
