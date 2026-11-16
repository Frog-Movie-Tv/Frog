/** The palette's shortcut as this keyboard writes it. */
export function paletteKey(platform: string): string {
	return /mac|iphone|ipad/i.test(platform) ? '⌘K' : 'Ctrl K';
}

/**
 * An iPad: Safari there asks for the Mac's pages and names itself a Mac, so
 * only the touch screen gives it away. The server cannot tell from the user
 * agent, so this device says it (Settings → Account → Signed-in devices).
 */
export function isIPad(platform: string, maxTouchPoints: number): boolean {
	return /ipad/i.test(platform) || (/^mac/i.test(platform) && maxTouchPoints > 1);
}
