// The one undo (decision 0026): every action that changes playlists, crates or Incoming says what it did in the toast,
// with Undo when the server gave it undo tokens. Undo shows "Undone." with Redo; Redo shows the action's own words
// again, with Undo. Offered only there, right after the action: there is no list of changes to undo later.
import { undo } from './api';
import { refresh } from './live.svelte';
import { toast } from './toast.svelte';

/** Bumped after every undo and redo: views showing what may have changed (a playlist's rows) read it again. */
export const undos = $state({ n: 0 });

type Options = {
  /** What Undo's toast says; "Undone." when left out. */
  undone?: string;
  /** After an undo (`true`) or a redo (`false`): the page follows (a deleted playlist's pin, a viewed playlist…). */
  after?: (undone: boolean) => unknown;
};

/** An action's toast: `text`, with Undo when it returned tokens (one, or one per playlist of an action on several;
 *  missing ones are left out). */
export function withUndo(text: string, tokens: (string | null | undefined)[], opts: Options = {}) {
  const t = tokens.filter((x): x is string => !!x);
  toast.show({ text, act: t.length ? { label: 'Undo', run: () => step(t, text, opts, true) } : undefined });
}

async function step(tokens: string[], text: string, opts: Options, undoing: boolean) {
  let worked = false;
  try {
    const { redo } = await undo(tokens);
    worked = true;
    if (undoing) toast.show({ text: opts.undone ?? 'Undone.', act: redo.length ? { label: 'Redo', run: () => step(redo, text, opts, false) } : undefined });
    else withUndo(text, redo, opts);
  } catch (e) {
    toast.fail(e, undoing ? 'Could not undo it.' : 'Could not redo it.');
  } finally {
    if (worked) await opts.after?.(undoing);
    undos.n++;
    refresh();
  }
}
