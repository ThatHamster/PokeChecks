import { moveInfo } from '../data/engineTypes';
import { useData } from '../data/store';
import { TypeBadge } from './bits';

/** A move with its type badge, as on the website's team builder. */
export function MoveName({ name }: { name: string }) {
  const { engine } = useData();
  const info = engine ? moveInfo(engine, name) : null;
  return (
    <span className="move-name">
      {info && <TypeBadge type={info.move_type} mini />}
      {name}
    </span>
  );
}
