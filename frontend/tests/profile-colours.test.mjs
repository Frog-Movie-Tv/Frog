// Every profile colour reads on every theme. Run with `npm test`.
//
// The same bar the themes keep: the ink on the colour (a button's label) and
// the colour on the page (a link, a figure) both at 4.5:1 or better, on the
// page background of every theme of that tone.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
	INK,
	PALETTE,
	PROFILE_COLOURS,
	PROFILE_SHADES,
	profileStyle
} from '../src/lib/profile-colours.ts';

const here = new URL('..', import.meta.url);
const css = readFileSync(new URL('src/app.css', here), 'utf8');
const settings = readFileSync(new URL('src/lib/settings.svelte.ts', here), 'utf8');

/** Each theme's tone, from THEMES, and its page background, from app.css. */
const themes = [...settings.matchAll(/id: '([\w-]+)',[^}]*?tone: '(light|dark)'/g)].map(
	([, id, tone]) => {
		const block =
			id === 'glass'
				? css.slice(css.indexOf(':root {'))
				: css.slice(css.indexOf(`:root[data-theme='${id}']`));
		const bg = block.match(/--bg: (#[0-9a-f]{6})/i)?.[1];
		return { id, tone, bg };
	}
);

function luminance(hex) {
	const [r, g, b] = [1, 3, 5].map((i) => {
		const v = parseInt(hex.slice(i, i + 2), 16) / 255;
		return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

test('every theme was found, with its background', () => {
	assert.equal(themes.length, 7);
	for (const t of themes) assert.match(t.bg ?? '', /^#/, t.id);
});

test('ten colours, three shades, two tones', () => {
	assert.equal(PROFILE_COLOURS.length, 10);
	for (const c of PROFILE_COLOURS)
		for (const s of PROFILE_SHADES)
			for (const tone of ['light', 'dark']) assert.match(PALETTE[c][s][tone], /^#[0-9a-f]{6}$/);
});

test('the ink on each colour, and each colour on each page, reads', () => {
	const weak = [];
	for (const c of PROFILE_COLOURS)
		for (const s of PROFILE_SHADES)
			for (const t of themes) {
				const accent = PALETTE[c][s][t.tone];
				const ink = contrast(accent, INK[t.tone]);
				const page = contrast(accent, t.bg);
				if (ink < 4.5) weak.push(`${c} ${s} ink on ${t.id}: ${ink.toFixed(2)}`);
				if (page < 4.5) weak.push(`${c} ${s} on ${t.id}'s page: ${page.toFixed(2)}`);
			}
	assert.deepEqual(weak, []);
});

test('an unknown colour leaves the theme alone', () => {
	assert.equal(profileStyle(null, null, 'dark'), undefined);
	assert.equal(profileStyle('fuchsia', 'light', 'dark'), undefined);
	assert.match(profileStyle('teal', 'nonsense', 'light'), /--accent: #04625e/);
});
