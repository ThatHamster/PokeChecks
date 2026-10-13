// Builds crates/wasm into src/wasm, where the app imports it from. Needs the Rust
// wasm32-unknown-unknown target and wasm-bindgen-cli 0.2.129 (see the repo README).

import { execFileSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(app, '..');
const out = join(app, 'src', 'wasm');

function run(cmd, args) {
  try {
    execFileSync(cmd, args, { cwd: repo, stdio: 'inherit' });
  } catch (err) {
    if (err.code === 'ENOENT') {
      console.error(`\n${cmd} isn't installed. See "Code layout" in the repo README for setup.`);
    }
    process.exit(1);
  }
}

run('cargo', [
  'build',
  '-p',
  'pokechecks-wasm',
  '--target',
  'wasm32-unknown-unknown',
  '--profile',
  'wasm',
  '--locked',
]);
rmSync(out, { recursive: true, force: true });
run('wasm-bindgen', [
  join(repo, 'target', 'wasm32-unknown-unknown', 'wasm', 'pokechecks_wasm.wasm'),
  '--target',
  'web',
  '--out-dir',
  out,
]);
