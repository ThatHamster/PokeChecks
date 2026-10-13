import type { CapacitorConfig } from '@capacitor/cli';

const config: CapacitorConfig = {
  appId: 'com.pokechecks.app',
  appName: 'PokeChecks',
  webDir: 'dist',
  plugins: {
    SplashScreen: {
      // The app hides the splash itself once its first screen is drawn (see App.tsx), so
      // there's no blank WebView flash in between.
      launchAutoHide: false,
      launchFadeOutDuration: 250,
      backgroundColor: '#fff6e6',
      showSpinner: false,
    },
  },
};

export default config;
