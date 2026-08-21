import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  server: {
    strictPort: true,
  },
  envPrefix: ['VITE_'],
  build: {
    target: 'es2022',
    minify: 'oxc',
    sourcemap: false,
    rollupOptions: {
      input: {
        main: 'index.html',
        about: 'about.html',
        titlebar: 'titlebar.html',
        update: 'update.html',
      },
    },
  },
});
