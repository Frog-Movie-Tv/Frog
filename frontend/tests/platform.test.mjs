// The palette's shortcut hint. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isIPad, paletteKey } from '../src/lib/platform.ts';

test('a Mac says ⌘K, where the shortcut already works', () => {
	assert.equal(paletteKey('MacIntel'), '⌘K');
	assert.equal(paletteKey('iPad'), '⌘K');
});

test('everything else says Ctrl K', () => {
	assert.equal(paletteKey('Win32'), 'Ctrl K');
	assert.equal(paletteKey('Linux x86_64'), 'Ctrl K');
	assert.equal(paletteKey(''), 'Ctrl K');
});

test('an iPad is told apart from a Mac by its touch screen', () => {
	assert.equal(isIPad('MacIntel', 5), true, 'iPadOS Safari names itself a Mac');
	assert.equal(isIPad('iPad', 5), true);
	assert.equal(isIPad('MacIntel', 0), false, 'a Mac');
	assert.equal(isIPad('Win32', 10), false, 'a touch laptop');
	assert.equal(isIPad('iPhone', 5), false);
});
