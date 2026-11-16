// The glow behind the page, and the themes Frog no longer has. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GLOWS, isGlow, moveTheme, rooms, swatch } from '../src/lib/glow.ts';

test('a glow of its own is three colours, stronger on a light page', () => {
	assert.equal(rooms('theme', 'dark'), null);
	assert.equal(rooms(undefined, 'light'), null);
	const dark = rooms('green', 'dark');
	const light = rooms('green', 'light');
	assert.equal(dark?.length, 3);
	const alpha = (c) => Number(c.match(/, ([\d.]+)\)$/)[1]);
	for (let i = 0; i < 3; i++) assert.ok(alpha(light[i]) > alpha(dark[i]));
	assert.deepEqual(rooms('off', 'dark'), ['transparent', 'transparent', 'transparent']);
});

test('every glow but the theme’s own and none has a swatch', () => {
	for (const g of GLOWS) {
		if (g.id === 'theme' || g.id === 'off') assert.equal(swatch(g.id), null);
		else assert.match(swatch(g.id) ?? '', /radial-gradient/);
	}
	assert.ok(isGlow('teal'));
	assert.ok(!isGlow('pond'));
});

test('a retired theme moves to its nearest, and AMOLED stays dark', () => {
	assert.deepEqual(moveTheme({ theme: 'pond' }), { theme: 'glass' });
	assert.deepEqual(moveTheme({ theme: 'pond-light' }), { theme: 'glass-light' });
	assert.deepEqual(moveTheme({ theme: 'amoled' }), { theme: 'dark', glow: 'off' });
	// A glow already chosen is kept.
	assert.deepEqual(moveTheme({ theme: 'amoled', glow: 'red' }), { theme: 'dark', glow: 'red' });
	const kept = { theme: 'light', glow: 'gold' };
	assert.equal(moveTheme(kept), kept);
	assert.deepEqual(moveTheme({}), {});
});
