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
		client: {start:"_app/immutable/entry/start.DF0VD23-.js",app:"_app/immutable/entry/app.ao9OyE26.js",imports:["_app/immutable/entry/start.DF0VD23-.js","_app/immutable/chunks/Bl6-ECxi.js","_app/immutable/chunks/CwC9PgJr.js","_app/immutable/chunks/DMKrJgiX.js","_app/immutable/entry/app.ao9OyE26.js","_app/immutable/chunks/DMKrJgiX.js","_app/immutable/chunks/CwC9PgJr.js","_app/immutable/chunks/Bzak7iHL.js","_app/immutable/chunks/BJeaJRxb.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
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
