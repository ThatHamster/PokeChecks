import { App as CapApp } from '@capacitor/app';
import { SplashScreen } from '@capacitor/splash-screen';
import { useEffect, useState, type ReactNode } from 'react';

import { refresh, start, useData } from './data/store';
import { AboutScreen } from './ui/AboutScreen';
import { BuilderScreen } from './ui/BuilderScreen';
import { closeTopSheet } from './ui/Sheet';
import { UsageScreen } from './ui/UsageScreen';
import { Ball } from './ui/bits';

type Tab = 'usage' | 'builder' | 'about';

const TABS: { key: Tab; label: string; icon: ReactNode }[] = [
  {
    key: 'usage',
    label: 'Usage',
    icon: (
      <svg viewBox="0 0 24 24" width="22" height="22">
        <rect x="3" y="12" width="4" height="9" rx="1.5" />
        <rect x="10" y="7" width="4" height="14" rx="1.5" />
        <rect x="17" y="3" width="4" height="18" rx="1.5" />
      </svg>
    ),
  },
  {
    key: 'builder',
    label: 'Builder',
    icon: (
      <svg viewBox="0 0 24 24" width="22" height="22">
        <rect x="3" y="3" width="8" height="8" rx="2.5" />
        <rect x="13" y="3" width="8" height="8" rx="2.5" />
        <rect x="3" y="13" width="8" height="8" rx="2.5" />
        <path d="M17 13.5v7M13.5 17h7" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" fill="none" />
      </svg>
    ),
  },
  {
    key: 'about',
    label: 'About',
    icon: (
      <svg viewBox="0 0 24 24" width="22" height="22">
        <path d="M12 2.5l2.6 6.3 6.9.6-5.2 4.5 1.6 6.7L12 17l-5.9 3.6 1.6-6.7L2.5 9.4l6.9-.6z" />
      </svg>
    ),
  },
];

export function App() {
  const { status, offline, outdated, refreshing } = useData();
  const [tab, setTab] = useState<Tab>('usage');

  useEffect(() => {
    void start();
    // Coming back to the app is a good moment to look for new stats.
    const onVisible = () => document.visibilityState === 'visible' && void refresh();
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  }, []);

  // Keep the native splash up until there's something to draw, then fade it out. Saved data
  // usually loads in a moment; the timeout covers a slow first download, which has its own
  // loading screen.
  useEffect(() => {
    if (status !== 'loading') {
      void SplashScreen.hide();
      return;
    }
    const timer = setTimeout(() => void SplashScreen.hide(), 2500);
    return () => clearTimeout(timer);
  }, [status]);

  // Android's back button closes an open sheet, then goes back to Usage, then leaves the app.
  useEffect(() => {
    const listener = CapApp.addListener('backButton', () => {
      if (closeTopSheet()) return;
      if (tab !== 'usage') setTab('usage');
      else void CapApp.exitApp();
    });
    return () => void listener.then((l) => l.remove());
  }, [tab]);

  if (status === 'loading') {
    return (
      <div className="splash">
        <Ball spin />
        <p>Getting the latest stats…</p>
      </div>
    );
  }
  if (status === 'empty') {
    return (
      <div className="splash">
        <Ball />
        <h2>No stats yet</h2>
        <p>Connect to the internet once to download them. After that, PokeChecks works offline.</p>
        <button className="go" disabled={refreshing} onClick={() => void refresh()}>
          {refreshing ? 'Trying…' : 'Try again'}
        </button>
      </div>
    );
  }

  return (
    <div className="app">
      {outdated && (
        <p className="banner warn">A newer version of PokeChecks is needed for the latest stats. Update it from the app store.</p>
      )}
      {offline && !outdated && <p className="banner">Offline · showing your saved stats</p>}
      {tab === 'usage' && <UsageScreen onOpenBuilder={() => setTab('builder')} />}
      {tab === 'builder' && <BuilderScreen />}
      {tab === 'about' && <AboutScreen />}
      <nav className="tabbar" aria-label="Sections">
        {TABS.map((t) => (
          <button key={t.key} aria-current={tab === t.key ? 'page' : undefined} onClick={() => setTab(t.key)}>
            <span className="tab-icon" aria-hidden>
              {t.icon}
            </span>
            {t.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
