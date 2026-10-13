// Saved teams: save the current team under a name, open or delete earlier ones. They're
// kept on the phone, with no account and no limit beyond the phone's storage.

import { useState } from 'react';

import type { Member } from '../data/engineTypes';
import { deleteSaved, saveTeam, useTeams, type SavedTeam } from '../data/teams';
import type { FormatKey, Manifest } from '../data/types';
import { MonIcon } from './bits';
import { Sheet } from './Sheet';

export function SavedTeams({
  manifest,
  format,
  members,
  onOpen,
  onClose,
}: {
  manifest: Manifest;
  format: FormatKey;
  members: Member[];
  onOpen: (team: SavedTeam) => void;
  onClose: () => void;
}) {
  const { saved } = useTeams();
  const [name, setName] = useState('');
  const [note, setNote] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const label = (f: FormatKey) => manifest.formats.find((x) => x.key === f)?.label ?? f;

  const save = async () => {
    if (members.length === 0) {
      setNote('Add at least one Pokémon before saving.');
      return;
    }
    const trimmed = name.trim() || 'My team';
    const replacing = saved.some((t) => t.format === format && t.name === trimmed);
    await saveTeam(trimmed, format, members);
    setNote(replacing ? `Replaced “${trimmed}”.` : `Saved “${trimmed}”.`);
    setName('');
  };

  return (
    <Sheet title="Saved teams" onClose={onClose}>
      <div className="save-row">
        <input
          placeholder="Team name"
          value={name}
          maxLength={40}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && void save()}
          aria-label="Team name"
        />
        <button className="go" onClick={() => void save()}>
          Save team
        </button>
      </div>
      <p className="hint">Saving with a name you’ve used in this format replaces that team.</p>
      {note && (
        <p className="toast-inline" role="status">
          {note}
        </p>
      )}

      {saved.length === 0 ? (
        <p className="note">No saved teams yet.</p>
      ) : (
        <ul className="saved-list">
          {saved.map((t) => (
            <li key={t.id} className="saved-card">
              <button className="saved-open" onClick={() => onOpen(t)}>
                <span className="saved-name">{t.name}</span>
                <span className="muted small">
                  {label(t.format)} · {new Date(t.savedAt).toLocaleDateString('en', { day: 'numeric', month: 'short' })}
                </span>
                <span className="saved-mons">
                  {t.members.map((m, i) => (
                    <MonIcon key={i} name={m.name} />
                  ))}
                </span>
              </button>
              {confirming === t.id ? (
                <button className="ghost danger" onClick={() => void deleteSaved(t.id)}>
                  Delete?
                </button>
              ) : (
                <button className="ghost" aria-label={`Delete ${t.name}`} onClick={() => setConfirming(t.id)}>
                  Delete
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </Sheet>
  );
}
