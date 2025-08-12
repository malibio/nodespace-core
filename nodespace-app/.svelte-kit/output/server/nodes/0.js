

export const index = 0;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/_layout.svelte.js')).default;
export const universal = {
  "prerender": true,
  "ssr": false
};
export const universal_id = "src/routes/+layout.ts";
export const imports = ["_app/immutable/nodes/0.v-B1Mb-j.js","_app/immutable/chunks/Bzak7iHL.js","_app/immutable/chunks/DlAJEGHk.js","_app/immutable/chunks/DMKrJgiX.js","_app/immutable/chunks/BJz-SxDU.js"];
export const stylesheets = ["_app/immutable/assets/0.C1F9zjvX.css"];
export const fonts = [];
