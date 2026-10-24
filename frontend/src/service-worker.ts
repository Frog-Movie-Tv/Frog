/// <reference types="@sveltejs/kit" />
/// <reference lib="webworker" />

import { assets, immutable } from '$app/manifest';
import { version } from '$app/env';

/**
 * Offline strategy, phase 1.
 *
 *  - app shell and hashed assets: cache first, they are immutable per version
 *  - posters through /img: cache first, TMDB paths are content addressed
 *  - API reads: network first, falling back to the last good response, so the
 *    library and a title you have opened before still render on the train
 *  - API writes: network only; a queue for offline ticks lands in phase 3
 */

const sw = self as unknown as ServiceWorkerGlobalScope;

const SHELL = `frog-shell-${version}`;
const DATA = 'frog-data-v1';
const MEDIA = 'frog-media-v1';

const PRECACHE = [...immutable, ...assets].map((entry) => entry.path);

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(SHELL)
			// One bad asset must not sink the whole install, so they are added
			// individually and failures are tolerated.
			.then((cache) => Promise.allSettled(PRECACHE.map((path) => cache.add(path))))
			.then(() => sw.skipWaiting())
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) =>
				Promise.all(
					keys
						.filter((k) => k.startsWith('frog-shell-') && k !== SHELL)
						.map((k) => caches.delete(k))
				)
			)
			.then(() => sw.clients.claim())
	);
});

sw.addEventListener('fetch', (event) => {
	const { request } = event;
	if (request.method !== 'GET') return;

	const url = new URL(request.url);
	if (url.origin !== sw.location.origin) return;

	if (url.pathname.startsWith('/img/')) {
		event.respondWith(cacheFirst(request, MEDIA));
		return;
	}

	if (url.pathname.startsWith('/api/')) {
		// Auth status must never come from cache: a stale "authenticated" would
		// show library chrome to a signed-out visitor.
		if (url.pathname.startsWith('/api/auth/')) return;
		event.respondWith(networkFirst(request, DATA));
		return;
	}

	if (PRECACHE.includes(url.pathname)) {
		event.respondWith(cacheFirst(request, SHELL));
		return;
	}

	// Everything else is a client-side route: serve the shell.
	if (request.mode === 'navigate') {
		event.respondWith(navigate(request));
	}
});

async function cacheFirst(request: Request, cacheName: string): Promise<Response> {
	const cache = await caches.open(cacheName);
	const hit = await cache.match(request);
	if (hit) return hit;

	const response = await fetch(request);
	if (response.status === 200) {
		cache.put(request, response.clone());
	}
	return response;
}

async function networkFirst(request: Request, cacheName: string): Promise<Response> {
	const cache = await caches.open(cacheName);
	try {
		const response = await fetch(request);
		if (response.ok) cache.put(request, response.clone());
		return response;
	} catch (error) {
		const hit = await cache.match(request);
		if (hit) return hit;
		throw error;
	}
}

async function navigate(request: Request): Promise<Response> {
	try {
		return await fetch(request);
	} catch {
		const cache = await caches.open(SHELL);
		const shell = (await cache.match('/index.html')) ?? (await cache.match('/'));
		return shell ?? new Response('Offline', { status: 503, statusText: 'Offline' });
	}
}
