import { api, ApiError } from './api';
import type { AuthStatus } from './types';

/**
 * One source of truth for "who am I and is this instance set up yet". The
 * layout blocks rendering on the first load of this, which is what keeps a
 * logged-out visitor from ever seeing library chrome.
 */
class Session {
	status = $state<AuthStatus | null>(null);
	ready = $state(false);
	offline = $state(false);

	async refresh(): Promise<AuthStatus | null> {
		try {
			this.status = await api.status();
			this.offline = false;
		} catch (e) {
			this.offline = e instanceof ApiError && e.status === 0;
			this.status = null;
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
		try {
			await api.logout();
		} finally {
			await this.refresh();
		}
	}
}

export const session = new Session();
