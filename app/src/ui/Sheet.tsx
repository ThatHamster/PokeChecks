// A bottom sheet that slides up over the screen, like native pickers. The Android back
// button and the backdrop close the top one first (see closeTopSheet in App.tsx). Sheets
// render into <body>, because screens animate in with a transform, and a transformed parent
// would pin a fixed-position sheet to the screen instead of the viewport.

import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

const open: (() => void)[] = [];

/** Closes the most recently opened sheet; false when none is open. */
export function closeTopSheet(): boolean {
  const close = open.at(-1);
  close?.();
  return Boolean(close);
}

export function Sheet({
  title,
  onClose,
  children,
  footer,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const fn = () => close.current();
    open.push(fn);
    // The page behind shouldn't scroll while a sheet is up.
    document.body.classList.add('sheet-open');
    return () => {
      open.splice(open.indexOf(fn), 1);
      if (open.length === 0) document.body.classList.remove('sheet-open');
    };
  }, []);

  return createPortal(
    <div className="sheet-layer" role="presentation">
      <button className="backdrop" aria-label="Close" onClick={onClose} />
      <section className="sheet" role="dialog" aria-modal="true" aria-label={title}>
        <header className="sheet-head">
          <span className="grabber" aria-hidden />
          <h2>{title}</h2>
          <button className="icon-btn" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </header>
        <div className="sheet-body">{children}</div>
        {footer && <footer className="sheet-foot">{footer}</footer>}
      </section>
    </div>,
    document.body,
  );
}
