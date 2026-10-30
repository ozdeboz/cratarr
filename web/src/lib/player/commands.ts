// What this page asks of the player that plays, as a command (0.10: one protocol). The app this page runs in takes its
// ops (`{type: 'player', op, ...}`, every app version); a player of Cratarr's the page drives takes the server's
// PlayerCommand (lib/generated), made here from the same op. Pure, so the tests take it as it is.
import type { PlayerCommand } from '../generated/PlayerCommand';
import type { QueueTrack } from '../generated/QueueTrack';
import type { Repeat } from '../generated/Repeat';
import type { RadioBy } from '../generated/RadioBy';
import type { AutoplayMode } from '../generated/AutoplayMode';

type Args = Record<string, unknown>;
const num = (v: unknown) => (typeof v === 'number' ? v : undefined);
const str = (v: unknown) => (typeof v === 'string' ? v : undefined);
const bool = (v: unknown) => v === true;
const tracks = (v: unknown) => (Array.isArray(v) ? (v as QueueTrack[]) : []);
/** The fields given, without those not given (as the op's own spread left them out). */
const given = <T extends object>(o: T): T => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as T;

/** The command for op `op` with its `args` (as lib/localPlayer hands them); null for an op no player takes. */
export function commandOf(op: string, args: Args = {}): PlayerCommand | null {
  switch (op) {
    case 'queue':
      return given({ command: 'play' as const, tracks: tracks(args.tracks), index: num(args.index) ?? 0, position_ms: num(args.positionMs) ?? 0, name: str(args.name) || undefined });
    case 'seek':
      return given({ command: 'seek' as const, position_ms: num(args.positionMs) ?? 0, wind: bool(args.wind) || undefined });
    case 'play':
      return { command: 'resume' };
    case 'sleep':
      return given({ command: 'sleep' as const, at_ms: num(args.atMs) ?? null, end_of_track: bool(args.endOfTrack) || undefined });
    case 'add':
      return given({ command: 'add' as const, tracks: tracks(args.tracks), at: num(args.at), after: str(args.after) });
    case 'move':
      return given({ command: 'move' as const, from: num(args.from), to: num(args.to), entry: str(args.entry), after: str(args.after) });
    case 'remove':
    case 'jump':
      return given({ command: op, index: num(args.index), entry: str(args.entry) });
    case 'upcoming':
      return { command: 'upcoming', tracks: tracks(args.tracks) };
    case 'shuffle':
    case 'skip_favourites':
      return { command: op, on: bool(args.on) };
    case 'repeat':
      return { command: 'repeat', mode: args.mode as Repeat };
    case 'radio':
      return given({ command: 'radio' as const, on: bool(args.on), by: args.by as RadioBy | undefined, count: num(args.count) });
    case 'autoplay':
      return given({
        command: 'autoplay' as const,
        mode: args.mode as AutoplayMode,
        albums: typeof args.albums === 'boolean' ? args.albums : undefined,
        restart: typeof args.restart === 'boolean' ? args.restart : undefined,
        seed: Array.isArray(args.seed) ? (args.seed as string[]) : undefined,
        within: str(args.within),
      });
    case 'wind':
      return { command: 'wind', dir: str(args.dir) ?? '', phase: str(args.phase) ?? '' };
    case 'direction':
      return { command: 'direction', dir: str(args.dir) ?? '' };
    case 'toggle':
    case 'next':
    case 'previous':
    case 'stop':
      return { command: op };
    default:
      return null;
  }
}

/** Where this device plays, as kept (0.12: the media server's player, `remote`, went from Cratarr's mode; one kept
 *  from before plays here again, without a word). Anything else: not an output. */
export const savedOutput = (v: unknown): 'local' | `player:${string}` | null =>
  v === 'local' || v === 'remote' ? 'local' : typeof v === 'string' && v.startsWith('player:') && v.length > 'player:'.length ? (v as `player:${string}`) : null;

/** The page's output for an app's target (decision 0062), and back. An app still pointed at the media server's player
 *  (`server`, before 0.12) plays here. */
export const outputOfTarget = (target: string): 'local' | `player:${string}` =>
  target.startsWith('player:') && target.length > 'player:'.length ? (target as `player:${string}`) : 'local';
export const targetOfOutput = (output: 'local' | `player:${string}`) => (output.startsWith('player:') ? output : 'self');

/** Whether the page drives a player (decisions 0022, 0031): its own queue (this browser's or app's player, or one of
 *  Cratarr's players it drives). False: a browser that cannot play here with no player chosen, nowhere to play (0.12:
 *  no longer the media server's player). */
export const holdsQueue = (output: string, canPlayHere: boolean, driven: string | null) => (output === 'local' && canPlayHere) || driven !== null;
