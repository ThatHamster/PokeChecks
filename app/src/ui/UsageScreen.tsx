// The usage ladder: every Pokémon in the format, most used first. Tapping one opens its most
// common items, abilities, moves and spread.

import { useDeferredValue, useMemo, useState } from 'react';

import { useUsageBanner } from '../ads/ads';
import { teamOf } from '../data/engineTypes';
import { setFormat, useData } from '../data/store';
import { addMember, useTeams } from '../data/teams';
import type { Mon } from '../data/types';
import { ItemIcon, MonIcon, Types, monthName, pct } from './bits';

const STAT_LABELS = ['HP', 'Atk', 'Def', 'SpA', 'SpD', 'Spe'];

export function UsageScreen({ onOpenBuilder, onOpenAbout }: { onOpenBuilder: () => void; onOpenAbout: () => void }) {
  const { manifest, format, usage } = useData();
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<string | null>(null);
  const deferred = useDeferredValue(query);
  // The apps' one banner sits above the tab bar on this screen only.
  const bannerHeight = useUsageBanner();
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
        <button className="about-btn" aria-label="About PokeChecks" onClick={onOpenAbout}>
          i
        </button>
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
                onOpenBuilder={onOpenBuilder}
              />
            ))}
          </ol>
        )}
      </section>
      {bannerHeight > 0 && <div className="banner-space" style={{ height: bannerHeight + 8 }} aria-hidden />}
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
  onOpenBuilder,
}: {
  mon: Mon;
  index: number;
  open: boolean;
  onToggle: () => void;
  onOpenBuilder: () => void;
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
      {open && <MonDetail mon={mon} onOpenBuilder={onOpenBuilder} />}
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

/** Adds the Pokémon to the team being built in this format, or says why it can't. */
function AddToTeam({ mon, onOpenBuilder }: { mon: Mon; onOpenBuilder: () => void }) {
  const { engine, format } = useData();
  const { current } = useTeams();
  const members = current[format] ?? [];
  const [message, setMessage] = useState<string | null>(null);
  if (!engine) return null;
  const onTeam = members.some((m) => m.name === mon.name);

  const add = () => {
    const tried = teamOf(engine, format, [...members, { name: mon.name, custom: null }]);
    if (tried.members.length === members.length) {
      setMessage(members.length >= 6 ? 'Your team is full.' : 'Another form of this species is already on your team.');
      return;
    }
    addMember(format, mon.name);
    setMessage(`Added on its most common set (${members.length + 1} / 6).`);
  };

  return (
    <div className="add-to-team">
      {onTeam ? (
        <button className="go" onClick={onOpenBuilder}>
          On your team · open builder
        </button>
      ) : (
        <button className="go" onClick={add}>
          + Add to team
        </button>
      )}
      {message && <span className="muted small">{message}</span>}
    </div>
  );
}

function MonDetail({ mon, onOpenBuilder }: { mon: Mon; onOpenBuilder: () => void }) {
  const spread = mon.spreads[0];
  return (
    <div className="detail">
      <AddToTeam mon={mon} onOpenBuilder={onOpenBuilder} />
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
