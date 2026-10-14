// Moves an element to the end of the page, so a sheet opened inside a layer (the playlist layer, decision 0023) is not
// held under that layer's stacking: it shows over the mini-player and everything else, as a sheet should. Svelte no
// longer finds it where its block was, so it is removed here when the block goes (after its out transition).
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
