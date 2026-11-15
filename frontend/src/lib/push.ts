import { api } from './api';

/**
 * The account that switched notifications on in this browser. A browser holds
 * one subscription; re-sending it for whoever signs in next would quietly
 * subscribe them too, so only this account's is kept alive.
 */
const OWNER = 'frog.push.account';

export function claimPush(username: string | null) {
	try {
		if (username) localStorage.setItem(OWNER, username);
		else localStorage.removeItem(OWNER);
	} catch {
		// Private mode: Settings still re-sends it when opened.
	}
}

function claimed(): string | null {
	try {
		return localStorage.getItem(OWNER);
	} catch {
		return null;
	}
}

/**
 * Hand this browser's subscription to the server again, as Frog opens. The
 * server forgets a device its push service calls gone, or one that failed
 * twenty times in a row; until now only Settings sent it back, so a phone
 * could go quiet with nothing on it saying so.
 */
export async function keepSubscribed(username: string | null) {
	if (!username || claimed() !== username) return;
	if (!('serviceWorker' in navigator) || !('PushManager' in window)) return;
	if (!('Notification' in window) || Notification.permission !== 'granted') return;
	try {
		const reg = await navigator.serviceWorker.getRegistration();
		const sub = await reg?.pushManager.getSubscription();
		if (sub) await api.pushSubscribe(sub.toJSON());
	} catch {
		// Quiet: Settings shows whether notifications are on.
	}
}
