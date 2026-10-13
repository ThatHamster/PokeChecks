// Picks a Pokémon to add: teammate suggestions first, then the whole ladder with a search.

import { useDeferredValue, useMemo, useState } from 'react';

import type { TeamResult } from '../data/engineTypes';
import type { Mon } from '../data/types';
import { MonIcon, Types, pct } from './bits';
import { Sheet } from './Sheet';

export function Picker({
  mons,
  team,
  onPick,
  onClose,
}: {
  mons: Mon[];
  team: TeamResult;
  onPick: (name: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const deferred = useDeferredValue(query);
  const onTeam = new Set(team.sets.map((s) => s.name));
  const list = useMemo(() => {
    const q = deferred.trim().toLowerCase();
    return (q ? mons.filter((m) => m.name.toLowerCase().includes(q)) : mons).slice(0, 150);
  }, [mons, deferred]);
  const suggested = team.sets.length > 0 && !deferred ? team.suggestions : [];

  return (
    <Sheet title="Add Pokémon" onClose={onClose}>
      <div className="search sticky">
        <input
          type="search"
          placeholder="Search Pokémon"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Search Pokémon"
        />
      </div>
      {suggested.length > 0 && (
        <>
          <h3 className="sub">Often used with your team</h3>
          <ul className="pick-list">
            {suggested.map((s) => (
              <li key={s.name}>
                <button onClick={() => onPick(s.name)}>
                  <MonIcon name={s.name} />
                  <span className="who">
                    <span className="name">{s.name}</span>
                    <Types name={s.name} />
                  </span>
                  <span className="num muted">{pct(s.score * 100, 0)} of teams</span>
                </button>
              </li>
            ))}
          </ul>
          <h3 className="sub">Everything, most used first</h3>
        </>
      )}
      <ul className="pick-list">
        {list.map((m) => (
          <li key={m.name}>
            <button onClick={() => onPick(m.name)} disabled={onTeam.has(m.name)}>
              <span className="rank num">{m.rank}</span>
              <MonIcon name={m.name} />
              <span className="who">
                <span className="name">{m.name}</span>
                <Types name={m.name} />
              </span>
              <span className="num muted">{onTeam.has(m.name) ? 'On team' : pct(m.usage)}</span>
            </button>
          </li>
        ))}
      </ul>
      {list.length === 0 && <p className="note">No Pokémon in this month’s stats match “{query}”.</p>}
    </Sheet>
  );
}
