/** Load the native Node bridge from the separately extracted runtime. */
import { createRequire } from 'node:module';
import { dirname, resolve, join } from 'node:path';
const require = createRequire(import.meta.url);
export function useAdapter(library = process.env.MUDU_ADAPTER_LIBRARY) {
  if (!library) throw new Error('Set MUDU_ADAPTER_LIBRARY to the extracted runtime library');
  const path = resolve(library);
  const bridge = require(join(dirname(path), 'mudu_adapter.node'));
  const handle = bridge.open(path);
  return (operation, frame) => new Uint8Array(bridge.call(handle, operation, Buffer.from(frame)));
}
