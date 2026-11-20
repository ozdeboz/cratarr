/** When something was last used or seen (seconds): the time today, else the day. */
export function when(s: number): string {
  const d = new Date(s * 1000);
  return d.toDateString() === new Date().toDateString()
    ? d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** When a player was last seen: Online now, else when; one never seen is Offline, the app's word for a device not online. */
export const lastSeen = (online: boolean, seen: number) => (online ? 'Online' : seen ? when(seen) : 'Offline');
