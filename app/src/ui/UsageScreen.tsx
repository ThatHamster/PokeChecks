// The usage ladder: every Pokémon in the format, most used first. Tapping one opens its most
// common items, abilities, moves and spread.

import { useDeferredValue, useMemo, useState } from 'react';

import { setFormat, useData } from '../data/store';
import type { Mon } from '../data/types';
import { ItemIcon, MonIcon, Types, monthName, pct } from './bits';

const STAT_LABELS = ['HP', 'Atk', 'Def', 'SpA', 'SpD', 'Spe'];

export function UsageScreen() {
  const { manifest, format, usage } = useData();
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<string | null>(null);
  const deferred = useDeferredValue(query);
  const meta = usage[format];
  const info = manifest?.formats.find((f) => f.key === format);

  const mons = useMemo(() => {
    const q = deferred.trim().toLowerCase();
    const list = meta?.mons ?? [];
    return q ? list.filter((m) => m.name.toLowerCase().includes(q)) : list;
  }, [meta, deferred]);

  return (
    <main className="screen">
      <header className="masthead">
        <h1>
          <span className="ball" aria-hidden />
          Poke<span className="accent">Checks</span>
        </h1>
        {manifest && info && (
          <p className="dek">
            <strong>Regulation {info.regulation}</strong> · Stats from {monthName(manifest.month)}
          </p>
        )}
      </header>

      {manifest && (
        <nav className="formats" aria-label="Format">
          {manifest.formats.map((f) => (
            <button
              key={f.key}
              className="chip"
              aria-pressed={f.key === format}
              onClick={() => {
                setOpen(null);
                void setFormat(f.key);
              }}
            >
              {f.label}
            </button>
          ))}
        </nav>
      )}

      <section className="pane">
        <div className="pane-head">
          <h2>Usage</h2>
          {meta && (
            <span className="label">
              {meta.battles.toLocaleString('en')} battles · {meta.cutoff}+
            </span>
          )}
        </div>
        <div className="search">
          <input
            type="search"
            placeholder="Search Pokémon"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Search Pokémon"
          />
        </div>
        {!meta ? (
          <p className="note">Loading {info?.label ?? 'this format'}…</p>
        ) : mons.length === 0 ? (
          <p className="note">No Pokémon in this month’s stats match “{query}”.</p>
        ) : (
          <ol className="ladder">
            {mons.slice(0, 120).map((m, i) => (
              <LadderRow
                key={m.name}
                mon={m}
                index={i}
                open={open === m.name}
                onToggle={() => setOpen(open === m.name ? null : m.name)}
              />
            ))}
          </ol>
        )}
      </section>
    </main>
  );
}

function Delta({ mon }: { mon: Mon }) {
  if (mon.prev === null) return <span className="delta flat">new</span>;
  const d = mon.usage - mon.prev;
  if (Math.abs(d) < 0.05) return <span className="delta flat num">±0.0</span>;
  return d > 0 ? (
    <span className="delta up num">▲ {d.toFixed(1)}</span>
  ) : (
    <span className="delta down num">▼ {Math.abs(d).toFixed(1)}</span>
  );
}

function LadderRow({
  mon,
  index,
  open,
  onToggle,
}: {
  mon: Mon;
  index: number;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    // Rows bounce in one after another, capped so long lists don't wait.
    <li className="row" style={{ animationDelay: `${Math.min(index, 14) * 28}ms` }}>
      <button className="row-head" aria-expanded={open} onClick={onToggle}>
        <span className="rank num">{mon.rank}</span>
        <MonIcon name={mon.name} />
        <span className="who">
          <span className="name">{mon.name}</span>
          <Types name={mon.name} />
        </span>
        <span className="usage">
          <span className="num strong">{pct(mon.usage)}</span>
          <Delta mon={mon} />
        </span>
      </button>
      <span className="bar" style={{ width: `${Math.min(mon.usage, 100)}%` }} aria-hidden />
      {open && <MonDetail mon={mon} />}
    </li>
  );
}

function ShareList({ title, shares, items = false }: { title: string; shares: Mon['items']; items?: boolean }) {
  return (
    <div className="shares">
      <span className="label">{title}</span>
      <ul>
        {shares.map((s) => (
          <li key={s.name}>
            <span className="share-name">
              {items && <ItemIcon item={s.name} />}
              {s.name}
            </span>
            <span className="num muted">{pct(s.pct)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function MonDetail({ mon }: { mon: Mon }) {
  const spread = mon.spreads[0];
  return (
    <div className="detail">
      <ShareList title="Items" shares={mon.items.slice(0, 4)} items />
      <ShareList title="Abilities" shares={mon.abilities.slice(0, 3)} />
      <ShareList title="Moves" shares={mon.moves.slice(0, 6)} />
      {spread && (
        <div className="shares">
          <span className="label">Top spread · {pct(spread.pct)}</span>
          <p className="spread">
            <strong>{spread.nature}</strong>{' '}
            {spread.points
              .map((p, i) => (p ? `${p} ${STAT_LABELS[i]}` : null))
              .filter(Boolean)
              .join(' / ') || 'No stat points'}
          </p>
        </div>
      )}
      {mon.teammates.length > 0 && (
        <div className="shares wide">
          <span className="label">Common teammates</span>
          <div className="mates">
            {mon.teammates.map((t) => (
              <span key={t} className="mate">
                <MonIcon name={t} />
                {t}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
