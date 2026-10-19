// The new sources (decision 0056, step 4) by type, for the renderer's layers.
import type { ModuleType, PictureModule } from '../patch';
import { SHAPES, Shapes } from './shapes';
import { WAVES, Waves } from './waves';

export const SOURCE_TYPES: ModuleType[] = [SHAPES, WAVES];

/** A running source of a type, compiled for this context; null for a type that is not one of these. */
export function createSource(type: string, gl: WebGL2RenderingContext): PictureModule | null {
  if (type === SHAPES.type) return new Shapes(gl);
  if (type === WAVES.type) return new Waves(gl);
  return null;
}
