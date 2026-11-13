// Everything the extension keeps, in the browser's own storage. Nothing here
// leaves the browser: settings, the site rules and the queue.

import { browser } from 'wxt/browser';
import type { Rules } from './rules.ts';
import { DEFAULT_SETTINGS, type Pending, type Settings } from './types.ts';

const local = () => browser.storage.local;

export async function settings(): Promise<Settings> {
	const got = (await local().get('settings')).settings as Partial<Settings> | undefined;
	return { ...DEFAULT_SETTINGS, ...got };
}

export async function saveSettings(change: Partial<Settings>): Promise<Settings> {
	const next = { ...(await settings()), ...change };
	await local().set({ settings: next });
	return next;
}

export async function rules(): Promise<Rules> {
	return ((await local().get('rules')).rules as Rules | undefined) ?? {};
}

export async function saveRules(next: Rules): Promise<void> {
	await local().set({ rules: next });
}

export async function queue(): Promise<Pending[]> {
	return ((await local().get('queue')).queue as Pending[] | undefined) ?? [];
}

export async function saveQueue(next: Pending[]): Promise<void> {
	await local().set({ queue: next });
}

/** Frog's address as it is stored: an origin, nothing after it. */
export function cleanUrl(typed: string): string | null {
	let t = typed.trim();
	if (!t) return null;
	if (!/^https?:\/\//i.test(t)) t = `https://${t}`;
	try {
		const u = new URL(t);
		return u.origin;
	} catch {
		return null;
	}
}
