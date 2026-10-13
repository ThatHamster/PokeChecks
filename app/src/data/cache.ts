// Downloads from the PokeChecks API, keeping a copy of every response on the device so the
// app works offline. Each copy keeps the server's ETag; asking again with it gets a
// 304 Not Modified (no download) when nothing changed.

import { get, set } from 'idb-keyval';

/** The live site by default; set VITE_API_BASE (e.g. http://127.0.0.1:3000) to use a local server. */
export const API_BASE: string = import.meta.env.VITE_API_BASE ?? 'https://pokechecks.onrender.com';

interface Saved<T> {
  etag: string | null;
  body: T;
  /** When the server last confirmed this copy, in ms since 1970. */
  checkedAt: number;
}

export interface Fetched<T> {
  body: T;
  /** False when the saved copy was still current or the network was unreachable. */
  changed: boolean;
  checkedAt: number;
  /** The server couldn't be reached, so this is the saved copy. */
  offline: boolean;
}

/** Thrown when there's no saved copy and the server can't be reached. */
export class NoDataError extends Error {
  constructor(path: string, cause: unknown) {
    super(`Couldn't download ${path}`, { cause });
    this.name = 'NoDataError';
  }
}

const key = (path: string) => `api:${path}`;

/** The saved copy only, without touching the network. */
export async function saved<T extends string | Blob>(path: string): Promise<T | undefined> {
  return ((await get(key(path))) as Saved<T> | undefined)?.body;
}

/**
 * The newest copy of `path`: downloaded if it changed, otherwise the saved one. Falls back
 * to the saved copy when offline.
 */
export async function fetchSaved<T extends string | Blob>(
  path: string,
  as: T extends Blob ? 'blob' : 'text',
): Promise<Fetched<T>> {
  const old = (await get(key(path))) as Saved<T> | undefined;
  let res: Response;
  try {
    res = await fetch(`${API_BASE}${path}`, {
      headers: old?.etag ? { 'If-None-Match': old.etag } : {},
      // The app keeps its own copy, so skip the WebView's HTTP cache.
      cache: 'no-store',
    });
  } catch (err) {
    if (old) return { body: old.body, changed: false, checkedAt: old.checkedAt, offline: true };
    throw new NoDataError(path, err);
  }

  const now = Date.now();
  if (res.status === 304 && old) {
    await set(key(path), { ...old, checkedAt: now });
    return { body: old.body, changed: false, checkedAt: now, offline: false };
  }
  if (!res.ok) {
    // A 502 means Smogon or Limitless is down; the saved copy is still good.
    if (old) return { body: old.body, changed: false, checkedAt: old.checkedAt, offline: true };
    throw new NoDataError(path, new Error(`HTTP ${res.status}`));
  }
  const body = (as === 'blob' ? await res.blob() : await res.text()) as T;
  await set(key(path), { etag: res.headers.get('ETag'), body, checkedAt: now } satisfies Saved<T>);
  return { body, changed: true, checkedAt: now, offline: false };
}
