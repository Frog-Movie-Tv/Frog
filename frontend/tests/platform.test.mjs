// The palette's shortcut hint. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { paletteKey } from '../src/lib/platform.ts';

test('a Mac says ⌘K, where the shortcut already works', () => {
	assert.equal(paletteKey('MacIntel'), '⌘K');
	assert.equal(paletteKey('iPad'), '⌘K');
});

test('everything else says Ctrl K', () => {
	assert.equal(paletteKey('Win32'), 'Ctrl K');
	assert.equal(paletteKey('Linux x86_64'), 'Ctrl K');
	assert.equal(paletteKey(''), 'Ctrl K');
});
