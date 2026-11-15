import type { Settings } from './types.ts';

/**
 * Which sites the extension watches. Either every site but the ignored ones,
 * or, with the allowlist on, only the allowed ones. A site on either list
 * covers its subdomains, so allowing max.com takes in play.max.com.
 */
type SiteSettings = Pick<Settings, 'allowOnly' | 'allowed' | 'ignored'>;

/** Whether `domain` is one of `list`, or a subdomain of one. */
export function covers(list: string[], domain: string): boolean {
	return list.some((d) => domain === d || domain.endsWith(`.${d}`));
}

/** Whether anything is detected on `domain`. A page with no site of its own is left alone. */
export function watching(s: SiteSettings, domain: string | null): boolean {
	if (!domain) return !s.allowOnly;
	return s.allowOnly ? covers(s.allowed, domain) : !covers(s.ignored, domain);
}

/**
 * What switching detection on or off for `domain` changes, in whichever
 * list the current mode uses. Off takes out every entry that covers it, so
 * the switch always ends where it was put.
 */
export function switchSite(
	s: SiteSettings,
	domain: string,
	on: boolean
): Partial<Pick<Settings, 'allowed' | 'ignored'>> {
	const without = (list: string[]) => list.filter((d) => !covers([d], domain));
	const add = (list: string[]) => [...new Set([...list, domain])].sort();
	if (s.allowOnly) return { allowed: on ? add(s.allowed) : without(s.allowed) };
	return { ignored: on ? without(s.ignored) : add(s.ignored) };
}
