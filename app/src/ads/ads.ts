// Ads for the Android and iOS apps (Google AdMob). The website has none. Adapted from
// Munitorum Ledger's ads module.
//
// - One banner, on the Usage screen only: never on the builder, calc, speed or teams
//   screens, which people use mid-match.
// - One optional "Watch a short ad" on About, in place of the website's Ko-fi link (the
//   app stores don't allow outside payment pages in apps).
// - Every request is non-personalized (npa), so iOS needs no "Allow tracking?" prompt.
//   Where the law requires it (EEA, UK, Switzerland) Google's consent form runs first, and
//   nothing is requested until it allows ads.

import { Capacitor } from '@capacitor/core';
import type { AdMobPlugin } from '@capacitor-community/admob';
import { useEffect, useState } from 'react';

// Google's published test ad units, used until the real ids are supplied at build time
// (VITE_ADMOB_* repository variables; see the README). Test ads are safe to tap; tapping
// real ads in your own builds can get the AdMob account suspended.
const TEST_UNITS = {
  android: { banner: 'ca-app-pub-3940256099942544/9214589741', rewarded: 'ca-app-pub-3940256099942544/5224354917' },
  ios: { banner: 'ca-app-pub-3940256099942544/2435281174', rewarded: 'ca-app-pub-3940256099942544/1712485313' },
} as const;

const env = import.meta.env;
const CONFIGURED = {
  android: { banner: env.VITE_ADMOB_ANDROID_BANNER as string | undefined, rewarded: env.VITE_ADMOB_ANDROID_REWARDED as string | undefined },
  ios: { banner: env.VITE_ADMOB_IOS_BANNER as string | undefined, rewarded: env.VITE_ADMOB_IOS_REWARDED as string | undefined },
};

/** The native apps show ads; the app running in a browser (development) never does. */
export const adsEnabled = Capacitor.isNativePlatform();

function unit(kind: 'banner' | 'rewarded') {
  const platform = Capacitor.getPlatform() === 'ios' ? 'ios' : 'android';
  const real = CONFIGURED[platform][kind];
  return { adId: real || TEST_UNITS[platform][kind], isTesting: !real, npa: true };
}

// ---------- Start-up and consent ----------

interface AdState {
  /** Google's consent SDK allows ad requests (consent given, or not needed here). */
  canRequestAds: boolean;
  /** The player's region requires a way to change their ad choices later. */
  privacyOptionsRequired: boolean;
}

let state: AdState = { canRequestAds: false, privacyOptionsRequired: false };
const listeners = new Set<(s: AdState) => void>();
// The plugin is wrapped in an object: Capacitor plugins are proxies that answer every
// property, `then` included, so resolving a promise with one makes JavaScript try to call
// `AdMob.then()`, which throws "not implemented".
let plugin: Promise<{ ad: AdMobPlugin }> | null = null;

function admob(): Promise<{ ad: AdMobPlugin }> {
  plugin ??= import('@capacitor-community/admob').then(async ({ AdMob, AdmobConsentStatus }) => {
    await AdMob.initialize();
    let consent = await AdMob.requestConsentInfo();
    if (consent.isConsentFormAvailable && consent.status === AdmobConsentStatus.REQUIRED) {
      consent = await AdMob.showConsentForm();
    }
    setState({
      canRequestAds: consent.canRequestAds,
      // The plugin doesn't export this enum; its value is the string "REQUIRED".
      privacyOptionsRequired: String(consent.privacyOptionsRequirementStatus) === 'REQUIRED',
    });
    return { ad: AdMob };
  });
  return plugin;
}

function setState(next: AdState) {
  state = next;
  listeners.forEach((l) => l(state));
}

/** Call once at start-up: asks for consent where required, then lets ads load. */
export function startAds() {
  if (adsEnabled) admob().catch((error) => console.warn('Ads unavailable:', error));
}

export function useAdState(): AdState {
  const [current, setCurrent] = useState(state);
  useEffect(() => {
    listeners.add(setCurrent);
    setCurrent(state);
    return () => {
      listeners.delete(setCurrent);
    };
  }, []);
  return current;
}

/** Lets the player revisit their ad choices (required where the consent form applies). */
export async function showAdPrivacyChoices() {
  const { ad } = await admob();
  await ad.showPrivacyOptionsForm();
  const { AdmobConsentStatus } = await import('@capacitor-community/admob');
  const consent = await ad.requestConsentInfo();
  setState({ ...state, canRequestAds: consent.canRequestAds || consent.status === AdmobConsentStatus.OBTAINED });
}

// ---------- Usage screen banner ----------

/** The tab bar's height plus its gap, matching `--tabbar` and its `bottom` offset in styles.css. */
const TAB_BAR_SPACE = 68 + 10 + 8;

/** The bottom safe-area inset in CSS pixels (the home indicator on newer iPhones). */
function safeAreaBottom(): number {
  const probe = document.createElement('div');
  probe.style.cssText = 'position:fixed;bottom:0;height:env(safe-area-inset-bottom);visibility:hidden';
  document.body.appendChild(probe);
  const inset = probe.getBoundingClientRect().height;
  probe.remove();
  return inset;
}

let bannerShown = false;

/**
 * Shows the banner just above the floating tab bar while the calling screen is open, and
 * returns its height so the screen can leave room for it.
 */
export function useUsageBanner(): number {
  const { canRequestAds } = useAdState();
  const [height, setHeight] = useState(0);
  useEffect(() => {
    if (!adsEnabled || !canRequestAds) return;
    let cancelled = false;
    let listener: { remove: () => Promise<void> } | undefined;
    (async () => {
      const { ad } = await admob();
      const { BannerAdPluginEvents, BannerAdPosition, BannerAdSize } = await import('@capacitor-community/admob');
      if (cancelled) return;
      listener = await ad.addListener(BannerAdPluginEvents.SizeChanged, (size) => !cancelled && setHeight(size.height));
      await ad.showBanner({
        ...unit('banner'),
        adSize: BannerAdSize.ADAPTIVE_BANNER,
        position: BannerAdPosition.BOTTOM_CENTER,
        margin: TAB_BAR_SPACE + safeAreaBottom(),
      });
      bannerShown = true;
    })().catch((error) => console.warn('Banner unavailable:', error));
    return () => {
      cancelled = true;
      void listener?.remove();
      if (bannerShown) {
        bannerShown = false;
        void admob().then(({ ad }) => ad.removeBanner());
      }
      setHeight(0);
    };
  }, [canRequestAds]);
  return height;
}

// ---------- Optional rewarded video ----------

/** Plays one optional video ad. Resolves true if it was watched to the end. */
export async function watchSupportAd(): Promise<boolean> {
  const { ad } = await admob();
  await ad.prepareRewardVideoAd(unit('rewarded'));
  const reward = await ad.showRewardVideoAd();
  return Boolean(reward);
}
