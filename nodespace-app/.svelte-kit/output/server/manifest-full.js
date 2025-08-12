export const manifest = (() => {
function __memo(fn) {
	let value;
	return () => value ??= (value = fn());
}

return {
	appDir: "_app",
	appPath: "_app",
	assets: new Set(["favicon.png","svelte.svg","tauri.svg","vite.svg"]),
	mimeTypes: {".png":"image/png",".svg":"image/svg+xml"},
	_: {
		client: {start:"_app/immutable/entry/start.B2WQ0D9q.js",app:"_app/immutable/entry/app.BqN5xxIQ.js",imports:["_app/immutable/entry/start.B2WQ0D9q.js","_app/immutable/chunks/D0yMlUVa.js","_app/immutable/chunks/CwC9PgJr.js","_app/immutable/chunks/DMKrJgiX.js","_app/immutable/entry/app.BqN5xxIQ.js","_app/immutable/chunks/DMKrJgiX.js","_app/immutable/chunks/CwC9PgJr.js","_app/immutable/chunks/Bzak7iHL.js","_app/immutable/chunks/BJeaJRxb.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
		nodes: [
			__memo(() => import('./nodes/0.js')),
			__memo(() => import('./nodes/1.js')),
			__memo(() => import('./nodes/2.js'))
		],
		remotes: {
			
		},
		routes: [
			{
				id: "/",
				pattern: /^\/$/,
				params: [],
				page: { layouts: [0,], errors: [1,], leaf: 2 },
				endpoint: null
			}
		],
		prerendered_routes: new Set([]),
		matchers: async () => {
			
			return {  };
		},
		server_assets: {}
	}
}
})();
