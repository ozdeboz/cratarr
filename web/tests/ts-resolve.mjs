// The tests' loader (npm test): the app's own imports name no extension (Vite resolves them); here a relative one
// without one is tried as a .ts file, and so is a module of Svelte's runes named without its .ts (`../reach.svelte`).
import { register } from 'node:module';

const hook = `export async function resolve(specifier, context, next) {
  try {
    return await next(specifier, context);
  } catch (e) {
    if (specifier.startsWith('.') && (!/[.][a-z]+$/.test(specifier) || specifier.endsWith('.svelte'))) return next(specifier + '.ts', context);
    throw e;
  }
}`;
register('data:text/javascript,' + encodeURIComponent(hook));
