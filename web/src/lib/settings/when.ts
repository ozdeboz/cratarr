/** When something was last used or seen, or a record went (seconds; SB13, RV8): the time today, else the day, with its year
 *  when that is not this year. */
export function when(s: number): string {
  const d = new Date(s * 1000);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', ...(d.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }) });
}

/** When a player was last seen: Online now, else when; one never seen is Offline, the app's word for a device not online. */
export const lastSeen = (online: boolean, seen: number) => (online ? 'Online' : seen ? when(seen) : 'Offline');
