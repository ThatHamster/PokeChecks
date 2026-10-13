// The calc's current matchup in each format, kept while the app is open so switching tabs
// doesn't lose it. The builder's "Calc" button sets it too.

import { useSyncExternalStore } from 'react';

import type { CalcRequest } from './engineTypes';
import type { FormatKey } from './types';

let state: Partial<Record<FormatKey, CalcRequest>> = {};
const listeners = new Set<() => void>();

export function useCalcState(): Partial<Record<FormatKey, CalcRequest>> {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

export function setCalc(format: FormatKey, req: CalcRequest) {
  state = { ...state, [format]: req };
  listeners.forEach((l) => l());
}

export function calcFor(format: FormatKey): CalcRequest | undefined {
  return state[format];
}
