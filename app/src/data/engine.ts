// Loads the WebAssembly build of pokechecks-core (crates/wasm), which runs the damage calc,
// team builder and game plans on the device. `npm run wasm` builds src/wasm.

import init, { Engine } from '../wasm/pokechecks_wasm.js';
import wasmUrl from '../wasm/pokechecks_wasm_bg.wasm?url';

export { Engine };

let ready: Promise<unknown> | null = null;

/** Starts the WebAssembly module once; later calls share the same start-up. */
export function loadWasm(): Promise<unknown> {
  ready ??= init({ module_or_path: wasmUrl });
  return ready;
}
