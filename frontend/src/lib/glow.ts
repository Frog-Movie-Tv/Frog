/*
 * The light behind the page. Every theme has three glows of its own
 * (`--room-1` to `-3` in app.css); a glow chosen in Appearance puts three of
 * one colour family there instead, on any theme, so the page can be tinted
 * without a theme of its own. Pure, so it is tested with node --test.
 */

export type Glow =
	'theme' | 'blue' | 'violet' | 'pink' | 'red' | 'orange' | 'gold' | 'green' | 'teal' | 'off';

type Rgb = [number, number, number];

/** Each glow as three related colours: the main light, and one to each side. */
export const GLOWS: { id: Glow; label: string; colours: [Rgb, Rgb, Rgb] | null }[] = [
	{ id: 'theme', label: 'The theme’s own', colours: null },
	{
		id: 'blue',
		label: 'Blue',
		colours: [
			[64, 120, 255],
			[90, 170, 255],
			[110, 90, 255]
		]
	},
	{
		id: 'violet',
		label: 'Violet',
		colours: [
			[140, 90, 255],
			[190, 100, 255],
			[90, 110, 255]
		]
	},
	{
		id: 'pink',
		label: 'Pink',
		colours: [
			[255, 90, 180],
			[220, 110, 255],
			[255, 140, 160]
		]
	},
	{
		id: 'red',
		label: 'Red',
		colours: [
			[255, 70, 80],
			[255, 120, 70],
			[220, 60, 140]
		]
	},
	{
		id: 'orange',
		label: 'Orange',
		colours: [
			[255, 140, 50],
			[255, 190, 70],
			[255, 90, 80]
		]
	},
	{
		id: 'gold',
		label: 'Gold',
		colours: [
			[255, 200, 60],
			[240, 230, 110],
			[255, 150, 60]
		]
	},
	{
		id: 'green',
		label: 'Green',
		colours: [
			[80, 210, 110],
			[150, 220, 90],
			[40, 190, 160]
		]
	},
	{
		id: 'teal',
		label: 'Teal',
		colours: [
			[40, 200, 190],
			[60, 170, 255],
			[80, 220, 140]
		]
	},
	{ id: 'off', label: 'No glow', colours: null }
];

/** How strongly each of the three shows: a light page needs more to show at all. */
const STRENGTH = { dark: [0.4, 0.3, 0.22], light: [0.5, 0.42, 0.36] } as const;

export function isGlow(value: unknown): value is Glow {
	return GLOWS.some((g) => g.id === value);
}

/**
 * The three glows to draw, or null for the theme's own. "No glow" is three
 * transparent ones: the page's own colour, as AMOLED had it.
 */
export function rooms(glow: Glow | undefined, tone: 'dark' | 'light'): string[] | null {
	if (glow === 'off') return ['transparent', 'transparent', 'transparent'];
	const colours = GLOWS.find((g) => g.id === glow)?.colours;
	if (!colours) return null;
	return colours.map(([r, g, b], i) => `rgba(${r}, ${g}, ${b}, ${STRENGTH[tone][i]})`);
}

/** A swatch for a glow: its three colours as they lie on the page. */
export function swatch(glow: Glow): string | null {
	const colours = GLOWS.find((g) => g.id === glow)?.colours;
	if (!colours) return null;
	const [a, b, c] = colours.map(([r, g, bl]) => `rgb(${r} ${g} ${bl})`);
	return `radial-gradient(circle at 25% 20%, ${a}, transparent 70%), radial-gradient(circle at 85% 30%, ${b}, transparent 65%), radial-gradient(circle at 60% 100%, ${c}, transparent 70%), #111`;
}

/**
 * Themes Frog no longer has, and where each one's people go: the nearest of
 * the four, and AMOLED with its glow off, so it stays as dark as it was.
 */
const MOVED: Record<string, { theme: string; glow?: Glow }> = {
	pond: { theme: 'glass' },
	'pond-light': { theme: 'glass-light' },
	amoled: { theme: 'dark', glow: 'off' }
};

/** Saved settings with a retired theme moved to its nearest; anything else as it was. */
export function moveTheme<T extends { theme?: unknown; glow?: unknown }>(saved: T): T {
	const moved = typeof saved.theme === 'string' ? MOVED[saved.theme] : undefined;
	if (!moved) return saved;
	const out = { ...saved, theme: moved.theme };
	if (moved.glow && !isGlow(saved.glow)) out.glow = moved.glow;
	return out;
}
