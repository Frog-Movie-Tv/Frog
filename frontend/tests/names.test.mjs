// Counting a name the way a person does. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { characters, nameLimit, USERNAME_MAX } from '../src/lib/names.ts';

const FLAG = '\u{1F1E7}\u{1F1EA}';
const FAMILY = '\u{1F468}‍\u{1F469}‍\u{1F467}‍\u{1F466}';

test('a flag, a family and an accent are one character each', () => {
	assert.equal(characters(FLAG), 1);
	assert.equal(characters(FAMILY), 1);
	assert.equal(characters('é'), 1);
	assert.equal(characters('Zjef'), 4);
	assert.equal(characters(''), 0);
});

test('an ordinary name stops at the limit', () => {
	assert.equal(nameLimit(''), USERNAME_MAX);
	assert.equal(nameLimit('Zjef'), USERNAME_MAX);
});

test('the field leaves room for what an emoji takes up', () => {
	assert.equal(nameLimit(FAMILY), USERNAME_MAX + FAMILY.length - 1);
	const full = FAMILY.repeat(USERNAME_MAX);
	assert.equal(characters(full), USERNAME_MAX);
	assert.equal(full.length, nameLimit(full), 'one more does not fit');
});
