import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { startAds } from './ads/ads';
import './styles.css';

// Asks for ad consent where the law requires it; does nothing outside the Android and iOS apps.
startAds();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
