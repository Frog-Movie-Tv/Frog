/**
 * The colours a profile can wear. A fixed list: ten colours, three shades
 * each, every one with a value for the light themes and one for the dark,
 * so the text on it always reads (tests/profile-colours.test.mjs checks each
 * pair). The server keeps only the two words; the values live here.
 *
 * A profile's colour is scoped to its own page (`profileStyle`), never the
 * app's theme: whoever opens the profile sees it in that person's colour.
 */

export const PROFILE_COLOURS = [
	'frog',
	'teal',
	'blue',
	'violet',
	'pink',
	'red',
	'orange',
	'amber',
	'brown',
	'grey'
] as const;
export const PROFILE_SHADES = ['light', 'medium', 'dark'] as const;

export type ProfileColour = (typeof PROFILE_COLOURS)[number];
export type ProfileShade = (typeof PROFILE_SHADES)[number];
export type Tone = 'light' | 'dark';

export const COLOUR_NAMES: Record<ProfileColour, string> = {
	frog: 'Frog green',
	teal: 'Teal',
	blue: 'Blue',
	violet: 'Violet',
	pink: 'Pink',
	red: 'Red',
	orange: 'Orange',
	amber: 'Amber',
	brown: 'Brown',
	grey: 'Grey'
};

export const SHADE_NAMES: Record<ProfileShade, string> = {
	light: 'Light',
	medium: 'Medium',
	dark: 'Dark'
};

/**
 * Each colour's value on the light themes (dark enough for white text, and
 * as text on the page) and on the dark themes (light enough for near-black
 * text, and as text on the page).
 */
export const PALETTE: Record<ProfileColour, Record<ProfileShade, Record<Tone, string>>> = {
	frog: {
		light: { light: '#017a1e', dark: '#74df7a' },
		medium: { light: '#026618', dark: '#4fbb58' },
		dark: { light: '#004e10', dark: '#299a38' }
	},
	teal: {
		light: { light: '#097571', dark: '#50ddd5' },
		medium: { light: '#04625e', dark: '#14b9b2' },
		dark: { light: '#014b48', dark: '#089691' }
	},
	blue: {
		light: { light: '#046abe', dark: '#a1ccfe' },
		medium: { light: '#0258a0', dark: '#5baafe' },
		dark: { light: '#00437d', dark: '#2389e9' }
	},
	violet: {
		light: { light: '#7352cb', dark: '#cbc1ff' },
		medium: { light: '#633eb7', dark: '#ac97fe' },
		dark: { light: '#4f269e', dark: '#9071ee' }
	},
	pink: {
		light: { light: '#b6357e', dark: '#feb3d6' },
		medium: { light: '#a11d6c', dark: '#fe77bd' },
		dark: { light: '#830055', dark: '#da569d' }
	},
	red: {
		light: { light: '#c62c30', dark: '#feb7b0' },
		medium: { light: '#b00a1d', dark: '#ff8078' },
		dark: { light: '#8a0013', dark: '#ea514f' }
	},
	orange: {
		light: { light: '#aa4e02', dark: '#ffb992' },
		medium: { light: '#8e4103', dark: '#fd8537' },
		dark: { light: '#6e3101', dark: '#d86501' }
	},
	amber: {
		light: { light: '#8b6103', dark: '#ffbd49' },
		medium: { light: '#745004', dark: '#db9901' },
		dark: { light: '#5a3d00', dark: '#b27c02' }
	},
	brown: {
		light: { light: '#8a5f42', dark: '#efbf9f' },
		medium: { light: '#774e31', dark: '#cb9c7d' },
		dark: { light: '#60391c', dark: '#aa7d5f' }
	},
	grey: {
		light: { light: '#656970', dark: '#c4c9d1' },
		medium: { light: '#53575e', dark: '#a2a6ae' },
		dark: { light: '#3f4349', dark: '#82878e' }
	}
};

/** What sits on the colour: white on the light themes, near-black on the dark. */
export const INK: Record<Tone, string> = { light: '#ffffff', dark: '#0b0b10' };

export const isColour = (v: unknown): v is ProfileColour =>
	typeof v === 'string' && (PROFILE_COLOURS as readonly string[]).includes(v);
export const isShade = (v: unknown): v is ProfileShade =>
	typeof v === 'string' && (PROFILE_SHADES as readonly string[]).includes(v);

/** The colour as a theme draws it: the accent, its ink and its soft tint. */
export function profileTokens(colour: ProfileColour, shade: ProfileShade, tone: Tone) {
	const accent = PALETTE[colour][shade][tone];
	return {
		accent,
		ink: INK[tone],
		soft: `color-mix(in srgb, ${accent} ${tone === 'light' ? 14 : 18}%, transparent)`
	};
}

/**
 * The CSS that puts a profile's colour on its page's root, or nothing for a
 * profile that keeps the theme's own.
 */
export function profileStyle(
	colour: string | null | undefined,
	shade: string | null | undefined,
	tone: Tone
): string | undefined {
	if (!isColour(colour)) return undefined;
	const t = profileTokens(colour, isShade(shade) ? shade : 'medium', tone);
	return `--accent: ${t.accent}; --accent-ink: ${t.ink}; --accent-soft: ${t.soft};`;
}
