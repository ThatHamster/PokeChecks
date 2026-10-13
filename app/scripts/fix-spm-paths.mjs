// `cap sync` on Windows writes backslash paths into the iOS Swift package, which Xcode on a
// Mac can't read. Capacitor runs this after every sync (the `capacitor:sync:after` script),
// so the committed file always uses forward slashes.

import { existsSync, readFileSync, writeFileSync } from 'node:fs';

const file = 'ios/App/CapApp-SPM/Package.swift';
if (existsSync(file)) {
  const before = readFileSync(file, 'utf8');
  const after = before.replace(/path: "([^"]+)"/g, (_, p) => `path: "${p.replaceAll('\\', '/')}"`);
  if (after !== before) {
    writeFileSync(file, after);
    console.log(`Fixed backslash paths in ${file}`);
  }
}
