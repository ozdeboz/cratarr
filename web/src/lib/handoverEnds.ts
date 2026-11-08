// Where a move goes by Cratarr's own handover (the owner, 2026-11-08: This device chosen on the phone, and the tablet and the
// ThinkPad said nothing played, where showing the phone would be "seamless"): from the player the music plays on now to the one
// chosen, both Cratarr's and connected. A handover tells every screen showing the one it left to follow it there; a stop there
// and a play here tell none. Null where either end is not one of them: the page's own way then.

type Player = { name: string; online: boolean };

/** The two ends of a move to `output` by handover, or null: `driven` the player the page drives, else `self` (this device's own
 *  player, where the page plays here, `here`), and the one chosen (`player:<name>`, or this device's own for `local`). */
export function handoverEnds(output: string, driven: string | null, here: boolean, self: string | null, players: Player[]): { from: string; to: string } | null {
  const from = driven ?? (here ? self : null);
  const to = output.startsWith('player:') ? output.slice('player:'.length) : output === 'local' ? self : null;
  const connected = (name: string | null) => !!name && players.some((p) => p.name === name && p.online);
  return from && to && from !== to && connected(from) && connected(to) ? { from, to } : null;
}
