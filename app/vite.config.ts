import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  // Relative asset paths so the build works inside Capacitor.
  base: './',
  server: {
    port: Number(process.env.PORT) || 5190,
    // The fonts live in the repo's assets/ folder, shared with the website.
    fs: { allow: ['..'] },
  },
  build: { target: 'es2022' },
  plugins: [react()],
});
