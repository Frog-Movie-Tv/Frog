/** The palette's shortcut as this keyboard writes it. */
export function paletteKey(platform: string): string {
	return /mac|iphone|ipad/i.test(platform) ? '⌘K' : 'Ctrl K';
}
