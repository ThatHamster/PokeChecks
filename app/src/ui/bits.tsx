// Small pieces shared by every screen: sprites, item icons and type badges, drawn the same
// way as on the website.

import { memo } from 'react';

import { useData } from '../data/store';

/** Types whose badge colour needs dark text (matches the website's LIGHT_TYPES). */
const LIGHT_TYPES = new Set([
  'Electric', 'Ice', 'Ground', 'Normal', 'Fairy', 'Bug', 'Steel', 'Flying', 'Rock', 'Fighting',
  'Grass', 'Stellar',
]);

export function TypeBadge({ type, mini = false }: { type: string; mini?: boolean }) {
  const ink = LIGHT_TYPES.has(type) ? 'ink-dark' : 'ink-light';
  return (
    <span
      className={`ty ${ink}${mini ? ' mini' : ''}`}
      style={{ background: `var(--ty-${type.toLowerCase()})` }}
    >
      {type}
    </span>
  );
}

export function Types({ name }: { name: string }) {
  const { engine } = useData();
  const types = engine?.types(name) ?? [];
  return (
    <span className="types">
      {types.map((t) => (
        <TypeBadge key={t} type={t} mini />
      ))}
    </span>
  );
}

/** A Pokémon's mini sprite from Showdown's icon sheet (40×30 each). */
export const MonIcon = memo(function MonIcon({ name }: { name: string }) {
  const { engine, sheets } = useData();
  if (!engine || !sheets.pokemon) return <span className="mon-icon" aria-hidden />;
  const [x, y] = engine.icon(name);
  return (
    <span
      className="mon-icon"
      aria-hidden
      style={{ backgroundImage: `url(${sheets.pokemon})`, backgroundPosition: `-${x}px -${y}px` }}
    />
  );
});

/** An item's icon from Showdown's item sheet (24×24 each), or nothing for unknown items. */
export const ItemIcon = memo(function ItemIcon({ item }: { item: string }) {
  const { engine, sheets } = useData();
  const pos = engine?.itemIcon(item);
  if (!pos || !sheets.items) return null;
  return (
    <span
      className="item-icon"
      aria-hidden
      style={{ backgroundImage: `url(${sheets.items})`, backgroundPosition: `-${pos[0]}px -${pos[1]}px` }}
    />
  );
});

export function pct(n: number, digits = 1): string {
  return `${n.toFixed(digits)}%`;
}

/** `2026-09` → `September 2026`. */
export function monthName(month: string): string {
  const [y, m] = month.split('-').map(Number);
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString('en', {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  });
}

/** A Poké Ball drawn in CSS, used as the logo and the loading spinner. */
export function Ball({ spin = false }: { spin?: boolean }) {
  return <span className={`ball${spin ? ' spin' : ''}`} aria-hidden />;
}
