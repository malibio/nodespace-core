type DynamicRoutes = {
	
};

type Layouts = {
	"/": undefined
};

export type RouteId = "/";

export type RouteParams<T extends RouteId> = T extends keyof DynamicRoutes ? DynamicRoutes[T] : Record<string, never>;

export type LayoutParams<T extends RouteId> = Layouts[T] | Record<string, never>;

export type Pathname = "/";

export type ResolvedPathname = `${"" | `/${string}`}${Pathname}`;

export type Asset = "/favicon.png" | "/svelte.svg" | "/tauri.svg" | "/vite.svg";