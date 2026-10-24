import { browser } from '$app/env';

export type OfflineState =
	| { status: 'pending' }
	| { status: 'ready'; scope: string }
	| { status: 'unavailable'; reason: string; detail?: string };

/**
 * Owns service worker registration.
 *
 * SvelteKit will register the worker for you, but it does it with a bare
 * promise: when the browser refuses, the rejection is uncaught and all you get
 * is a red line in the console. Registration is done here instead so a refusal
 * becomes a fact the app can state plainly.
 *
 * Browsers refuse for exactly one reason in practice: service workers need a
 * secure context. `localhost` counts, a plain-http LAN or tailnet address does
 * not, which is why reaching Frog over HTTPS matters.
 */
class Offline {
	state = $state<OfflineState>({ status: 'pending' });

	get ready() {
		return this.state.status === 'ready';
	}

	async register() {
		if (!browser) return;

		if (!('serviceWorker' in navigator)) {
			this.state = {
				status: 'unavailable',
				reason: 'This browser has no service worker support.'
			};
			return;
		}

		if (!window.isSecureContext) {
			this.state = {
				status: 'unavailable',
				reason: 'Offline mode needs a secure connection.',
				detail: `Frog is being served over ${location.protocol}//${location.host}. Put Tailscale Serve in front of it for HTTPS, or open it on localhost.`
			};
			return;
		}

		try {
			const registration = await navigator.serviceWorker.register('/service-worker.js', {
				scope: '/'
			});
			this.state = { status: 'ready', scope: registration.scope };
		} catch (error) {
			this.state = {
				status: 'unavailable',
				reason: 'This browser would not install the offline worker.',
				detail: error instanceof Error ? error.message : String(error)
			};
		}
	}
}

export const offline = new Offline();
