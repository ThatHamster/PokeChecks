// The match being tracked on the Speed screen, per format, kept while the app is open.

import { useSyncExternalStore } from 'react';

import type { MatchState } from './engineTypes';
import type { FormatKey } from './types';

export const emptyMatch = (): MatchState => ({
  foes: [],
  mine: [],
  trick_room: false,
  my_tailwind: false,
  foe_tailwind: false,
  my_drop: false,
  foe_drop: false,
  weather: null,
});

let state: Partial<Record<FormatKey, MatchState>> = {};
const listeners = new Set<() => void>();

export function useMatches(): Partial<Record<FormatKey, MatchState>> {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

export function setMatch(format: FormatKey, match: MatchState) {
  state = { ...state, [format]: match };
  listeners.forEach((l) => l());
}

/** Changes the match from its latest value, so quick taps never overwrite each other. */
export function updateMatch(format: FormatKey, change: (m: MatchState) => MatchState) {
  setMatch(format, change(state[format] ?? emptyMatch()));
}
