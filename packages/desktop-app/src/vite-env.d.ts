/// <reference types="vite/client" />

// Compile-time constant injected by Vite's `define` (see vite.config.js). Holds
// this package's own version (from package.json) so the app can report its
// frontend version without importing package.json at runtime.
declare const __APP_VERSION__: string;
