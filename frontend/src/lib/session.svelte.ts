import { browser } from '$app/env';
import { api, ApiError } from './api';
import { settings } from './settings.svelte';
import type { AuthStatus } from './types';

const REMEMBERED = 'frog.session.v1';

/**
 * One source of truth for "who am I and is this instance set up yet". The
 * layout blocks rendering on the first load of this, which is what keeps a
 * signed-out visitor from ever seeing library chrome.
 *
 * Offline, the status request cannot reach the server, and treating that as
 * "signed out" sent every page to the sign-in screen, which made offline
 * mode useless. So the last confirmed status is remembered, and used only
 * when the network is unreachable. A real answer from the server always
 * wins, and signing out forgets it.
 */
class Session {
	status = $state<AuthStatus | null>(null);
	ready = $state(false);
	offline = $state(false);

	async refresh(): Promise<AuthStatus | null> {
		try {
			this.status = await api.status();
			this.offline = false;
			this.remember(this.status);
		} catch (e) {
			const unreachable = e instanceof ApiError && e.status === 0;
			this.offline = unreachable;
			this.status = unreachable ? this.recall() : null;
		} finally {
			this.ready = true;
		}
		return this.status;
	}

	get authenticated() {
		return this.status?.authenticated === true;
	}

	get needsSetup() {
		return this.status?.needs_setup === true;
	}

	async logout() {
		await settings.flush();
		this.forget();
		// Take the offline copy of the library with us.
		if (browser) navigator.serviceWorker?.controller?.postMessage('frog:signed-out');
		try {
			await api.logout();
		} finally {
			await this.refresh();
			settings.forget();
		}
	}

	private remember(status: AuthStatus | null) {
		if (!browser) return;
		try {
			if (status?.authenticated) localStorage.setItem(REMEMBERED, JSON.stringify(status));
			else localStorage.removeItem(REMEMBERED);
		} catch {
			// Private mode: offline just will not survive a reload.
		}
	}

	private recall(): AuthStatus | null {
		if (!browser) return null;
		try {
			const raw = localStorage.getItem(REMEMBERED);
			return raw ? (JSON.parse(raw) as AuthStatus) : null;
		} catch {
			return null;
		}
	}

	private forget() {
		if (!browser) return;
		try {
			localStorage.removeItem(REMEMBERED);
			// Preferences belong to the account, not the device.
			localStorage.removeItem('frog.settings.v2');
		} catch {
			// Nothing to forget.
		}
	}
}

export const session = new Session();
