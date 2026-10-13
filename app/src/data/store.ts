// The app's data: what's saved on the device, kept up to date from the API. Screens read it
// through useData(). Saved data shows straight away; a refresh runs behind it and swaps in
// anything new.

import { useSyncExternalStore } from 'react';

import { NoDataError, fetchSaved, saved } from './cache';
import { Engine, loadWasm } from './engine';
import { SUPPORTED_SCHEMA, type FormatKey, type Manifest, type Usage } from './types';

export interface DataState {
  /** `loading` until saved data (or a first download) is ready; `empty` if neither worked. */
  status: 'loading' | 'ready' | 'empty';
  manifest: Manifest | null;
  format: FormatKey;
  /** Usage for each format loaded so far. */
  usage: Partial<Record<FormatKey, Usage>>;
  engine: Engine | null;
  /** Object URLs for the Pokémon and item icon sheets. */
  sheets: { pokemon: string | null; items: string | null };
  /** When the server last confirmed the data, in ms since 1970. */
  checkedAt: number | null;
  /** The last refresh couldn't reach the server. */
  offline: boolean;
  refreshing: boolean;
  /** The app is too old for the server's data and needs updating from the store. */
  outdated: boolean;
}

const FORMAT_KEY = 'pokechecks:format';

function savedFormat(): FormatKey {
  try {
    const f = localStorage.getItem(FORMAT_KEY);
    if (f === 'vgc' || f === 'vgc-bo3' || f === 'bss') return f;
  } catch {
    // Storage can be unavailable; VGC is a fine default.
  }
  return 'vgc';
}

let state: DataState = {
  status: 'loading',
  manifest: null,
  format: savedFormat(),
  usage: {},
  engine: null,
  sheets: { pokemon: null, items: null },
  checkedAt: null,
  offline: false,
  refreshing: false,
  outdated: false,
};

const listeners = new Set<() => void>();

function update(patch: Partial<DataState>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useData(): DataState {
  return useSyncExternalStore(subscribe, () => state);
}

// ---------- Loading ----------

/** The text of the dex, so a new Engine can be built when it changes. */
let dexJson: string | null = null;
/** The raw usage text per format, kept to feed a rebuilt Engine. */
const usageJson: Partial<Record<FormatKey, string>> = {};

function usagePath(manifest: Manifest, format: FormatKey): string {
  const info = manifest.formats.find((f) => f.key === format) ?? manifest.formats[0];
  return `/api/v1/usage/${info.key}?cutoff=${info.default_cutoff}`;
}

function sheetUrl(blob: Blob | undefined, previous: string | null): string | null {
  if (!blob) return previous;
  if (previous) URL.revokeObjectURL(previous);
  return URL.createObjectURL(blob);
}

/** Builds a fresh Engine from the saved dex and every loaded usage file. */
function rebuildEngine(): Engine | null {
  if (!dexJson) return null;
  state.engine?.free();
  const engine = new Engine(dexJson);
  for (const json of Object.values(usageJson)) {
    if (json) engine.addUsage(json);
  }
  return engine;
}

function setUsage(format: FormatKey, json: string) {
  usageJson[format] = json;
  const usage = JSON.parse(json) as Usage;
  state.engine?.addUsage(json);
  update({ usage: { ...state.usage, [format]: usage } });
}

/** Shows whatever is saved on the device, without waiting for the network. */
async function loadSavedData() {
  await loadWasm();
  const manifestText = await saved<string>('/api/v1/manifest');
  const dex = await saved<string>('/api/v1/dex');
  if (!manifestText || !dex) return;
  const manifest = JSON.parse(manifestText) as Manifest;
  if (manifest.schema !== SUPPORTED_SCHEMA) return;
  const usageText = await saved<string>(usagePath(manifest, state.format));
  if (!usageText) return;

  dexJson = dex;
  usageJson[state.format] = usageText;
  const [pokemon, items] = await Promise.all([
    saved<Blob>('/api/v1/sprites/pokemon.png'),
    saved<Blob>('/api/v1/sprites/items.png'),
  ]);
  update({
    manifest,
    engine: rebuildEngine(),
    usage: { [state.format]: JSON.parse(usageText) as Usage },
    sheets: {
      pokemon: sheetUrl(pokemon, state.sheets.pokemon),
      items: sheetUrl(items, state.sheets.items),
    },
    status: 'ready',
  });
}

let refreshing: Promise<void> | null = null;

/** Checks the server for newer data and swaps in anything that changed. */
export function refresh(): Promise<void> {
  refreshing ??= (async () => {
    update({ refreshing: true });
    try {
      await loadWasm();
      const m = await fetchSaved<string>('/api/v1/manifest', 'text');
      const manifest = JSON.parse(m.body) as Manifest;
      if (manifest.schema !== SUPPORTED_SCHEMA) {
        update({ outdated: true });
        return;
      }
      const [dex, usage, pokemon, items] = await Promise.all([
        fetchSaved<string>('/api/v1/dex', 'text'),
        fetchSaved<string>(usagePath(manifest, state.format), 'text'),
        fetchSaved<Blob>('/api/v1/sprites/pokemon.png', 'blob'),
        fetchSaved<Blob>('/api/v1/sprites/items.png', 'blob'),
      ]);
      // Tournament teams aren't shown yet, but saving them now means they work offline later.
      fetchSaved<string>('/api/v1/tournaments', 'text').catch(() => undefined);

      if (dex.changed || !state.engine) {
        dexJson = dex.body;
        update({ engine: rebuildEngine() });
      }
      if (usage.changed || !state.usage[state.format]) setUsage(state.format, usage.body);
      update({
        manifest,
        sheets: {
          pokemon: pokemon.changed || !state.sheets.pokemon ? sheetUrl(pokemon.body, state.sheets.pokemon) : state.sheets.pokemon,
          items: items.changed || !state.sheets.items ? sheetUrl(items.body, state.sheets.items) : state.sheets.items,
        },
        checkedAt: Math.min(m.checkedAt, dex.checkedAt, usage.checkedAt),
        offline: m.offline || dex.offline || usage.offline,
        status: 'ready',
      });
    } catch (err) {
      if (!(err instanceof NoDataError)) console.error(err);
      update({ offline: true, status: state.status === 'ready' ? 'ready' : 'empty' });
    } finally {
      update({ refreshing: false });
      refreshing = null;
    }
  })();
  return refreshing;
}

/** Switches format, loading its usage from the device or the server if needed. */
export async function setFormat(format: FormatKey) {
  try {
    localStorage.setItem(FORMAT_KEY, format);
  } catch {
    // Not remembering the choice is fine.
  }
  update({ format });
  const manifest = state.manifest;
  if (!manifest || state.usage[format]) return;
  const path = usagePath(manifest, format);
  const local = await saved<string>(path);
  if (local) setUsage(format, local);
  try {
    const fresh = await fetchSaved<string>(path, 'text');
    if (fresh.changed || !local) setUsage(format, fresh.body);
  } catch {
    if (!local) update({ offline: true });
  }
}

/** Starts the app: saved data first, then a check for anything newer. */
export async function start() {
  try {
    await loadSavedData();
  } catch (err) {
    // A damaged saved copy just means downloading again.
    console.error(err);
  }
  await refresh();
}
