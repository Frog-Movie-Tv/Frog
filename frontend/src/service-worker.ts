/// <reference types="@sveltejs/kit" />
/// <reference lib="webworker" />

import { assets, immutable } from '$app/manifest';
import { version } from '$app/env';

/**
 * Offline strategy.
 *
 *  - app shell and hashed assets: cache first, immutable per version
 *  - posters through /img: cache first, TMDB paths are content addressed
 *  - API reads, auth status included: network first, falling back to the
 *    last good response, so a library you have opened still renders offline
 *  - API writes: network only
 *
 * The manifest lists paths relative to the app root ("_app/…"), while request
 * URLs are absolute ("/_app/…"). Every path is normalised to the absolute
 * form once, here; comparing the two shapes directly is what used to make
 * every asset miss the cache.
 */

const sw = self as unknown as ServiceWorkerGlobalScope;

const SHELL = `frog-shell-${version}`;
const DATA = 'frog-data-v1';
const MEDIA = 'frog-media-v1';

/** The page every client-side route boots from. */
const APP_SHELL = '/';

const absolute = (path: string) => (path.startsWith('/') ? path : `/${path}`);
const PRECACHE = new Set([APP_SHELL, ...[...immutable, ...assets].map((e) => absolute(e.path))]);

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(SHELL)
			// One missing asset must not sink the whole install.
			.then((cache) => Promise.allSettled([...PRECACHE].map((path) => cache.add(path))))
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

	if (request.mode === 'navigate') {
		event.respondWith(navigate(request));
		return;
	}

	if (url.pathname.startsWith('/img/')) {
		event.respondWith(cacheFirst(request, MEDIA));
		return;
	}

	if (url.pathname.startsWith('/api/')) {
		// Exports are downloads, not data the app reads back.
		if (url.pathname.startsWith('/api/export/')) return;
		event.respondWith(networkFirst(request, DATA));
		return;
	}

	if (PRECACHE.has(url.pathname)) {
		event.respondWith(cacheFirst(request, SHELL));
	}
});

/**
 * Signing out has to take the cached data with it, or the next person to open
 * the browser offline would see the library.
 */
sw.addEventListener('message', (event) => {
	if (event.data === 'frog:signed-out') {
		event.waitUntil(caches.delete(DATA));
	}
});

interface PushMessage {
	title?: string;
	body?: string;
	url?: string;
	tag?: string;
	image?: string | null;
}

/**
 * A new episode, sent by the server's worker. The message arrives encrypted
 * and complete, so it shows even when Frog itself cannot be reached.
 */
sw.addEventListener('push', (event) => {
	let message: PushMessage = {};
	try {
		message = event.data?.json() ?? {};
	} catch {
		message = { body: event.data?.text() };
	}
	const options: NotificationOptions & { image?: string } = {
		body: message.body ?? 'New episodes are out',
		tag: message.tag,
		icon: '/icon-192.png',
		badge: '/badge-96.png',
		data: { url: message.url ?? '/' }
	};
	if (message.image) options.image = message.image;
	event.waitUntil(sw.registration.showNotification(message.title ?? 'Frog', options));
});

sw.addEventListener('notificationclick', (event) => {
	event.notification.close();
	const url = new URL((event.notification.data as { url?: string })?.url ?? '/', sw.location.origin)
		.href;
	event.waitUntil(
		sw.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windows) => {
			const open = windows.find((w) => new URL(w.url).origin === sw.location.origin);
			if (open) return open.navigate(url).then((w) => (w ?? open).focus());
			return sw.clients.openWindow(url);
		})
	);
});

/**
 * Browsers occasionally rotate a push subscription. Re-subscribe with the same
 * server key and tell Frog, or alerts would stop without a word.
 */
sw.addEventListener('pushsubscriptionchange', (event) => {
	const change = event as Event & {
		oldSubscription?: PushSubscription | null;
		newSubscription?: PushSubscription | null;
		waitUntil(p: Promise<unknown>): void;
	};
	change.waitUntil(
		(async () => {
			const key = change.oldSubscription?.options.applicationServerKey;
			const next =
				change.newSubscription ??
				(key
					? await sw.registration.pushManager.subscribe({
							userVisibleOnly: true,
							applicationServerKey: key
						})
					: null);
			if (!next) return;
			await fetch('/api/push/subscribe', {
				method: 'POST',
				credentials: 'same-origin',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(next.toJSON())
			});
			if (change.oldSubscription && change.oldSubscription.endpoint !== next.endpoint) {
				await fetch('/api/push/unsubscribe', {
					method: 'POST',
					credentials: 'same-origin',
					headers: { 'content-type': 'application/json' },
					body: JSON.stringify({ endpoint: change.oldSubscription.endpoint })
				});
			}
		})()
	);
});

async function cacheFirst(request: Request, cacheName: string): Promise<Response> {
	const cache = await caches.open(cacheName);
	const hit = await cache.match(request);
	if (hit) return hit;

	const response = await fetch(request);
	if (response.status === 200) cache.put(request, response.clone());
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

/** Every route is the same single page, so the cached shell answers them all. */
async function navigate(request: Request): Promise<Response> {
	try {
		return await fetch(request);
	} catch {
		const cache = await caches.open(SHELL);
		return (
			(await cache.match(APP_SHELL)) ??
			new Response('Frog is offline and has nothing cached yet.', {
				status: 503,
				headers: { 'content-type': 'text/plain; charset=utf-8' }
			})
		);
	}
}
