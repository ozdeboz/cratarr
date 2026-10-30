// The new sources (decision 0056, step 4) by type, for the renderer's layers.
import type { ModuleType, PictureModule } from '../patch';
import { GRADIENT, Gradient } from './gradient';
import { NOISE, Noise } from './noise';
import { IRREGULAR_SHAPES, IrregularShapes, SHAPES, Shapes } from './shapes';
import { WAVES, Waves } from './waves';

export const SOURCE_TYPES: ModuleType[] = [SHAPES, IRREGULAR_SHAPES, WAVES, NOISE, GRADIENT];

/** A running source of a type, compiled for this context; null for a type that is not one of these. `id`: its module's
 *  (what a seeded source draws by, alike on every device). */
export function createSource(type: string, gl: WebGL2RenderingContext, id = ''): PictureModule | null {
  if (type === SHAPES.type) return new Shapes(gl);
  if (type === IRREGULAR_SHAPES.type) return new IrregularShapes(gl);
  if (type === WAVES.type) return new Waves(gl);
  if (type === NOISE.type) return new Noise(gl, id);
  if (type === GRADIENT.type) return new Gradient(gl);
  return null;
}
