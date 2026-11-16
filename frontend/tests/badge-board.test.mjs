// The badges page's cards, filters and orders. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	cards,
	fromAddress,
	grouped,
	heldBy,
	highlights,
	narrow,
	sorted,
	toAddress
} from '../src/lib/badge-board.ts';

const held = (id, name, extra = {}) => ({
	id,
	name,
	description: `About ${name}`,
	awarded_at: '2026-11-10T10:00:00Z',
	shown: true,
	small: '',
	large: '',
	automatic: true,
	earned_on: '2026-11-10',
	group: 'watching',
	difficulty: 1,
	holders: 2,
	...extra
});
const locked = (id, name, extra = {}) => ({
	id,
	name,
	description: `About ${name}`,
	small: '',
	large: '',
	moment: false,
	ladder: null,
	step: null,
	progress: null,
	group: 'watching',
	difficulty: 3,
	holders: 0,
	...extra
});

const board = {
	earned: [
		held(1, 'Frogspawn', { ladder: 'films', step: 0, steps: 3 }),
		held(2, 'Nami Lover', {
			automatic: false,
			group: 'given',
			difficulty: null,
			earned_on: null,
			awarded_at: '2026-11-14T09:00:00Z',
			holders: 1
		}),
		held(3, 'Triple Bill', {
			group: 'moments',
			moment: true,
			difficulty: 2,
			earned_on: '2026-11-12'
		})
	],
	locked: [
		locked(4, 'Tadpole Matinee', {
			ladder: 'films',
			step: 1,
			steps: 3,
			progress: { have: 8, need: 10 }
		}),
		locked(5, 'Cinefrog', {
			ladder: 'films',
			step: 2,
			steps: 3,
			difficulty: 5,
			progress: { have: 8, need: 250 }
		}),
		locked(6, 'Hidden Gem', { group: 'taste', difficulty: 2 })
	],
	total: 6,
	accounts: 3
};

const names = (list) => list.map((c) => c.name);

test('every badge is one card, earned ones first', () => {
	const all = cards(board);
	assert.equal(all.length, 6);
	assert.deepEqual(
		all.map((c) => c.earned),
		[true, true, true, false, false, false]
	);
	assert.equal(all[1].day, '2026-11-14');
});

test('filters narrow by earned, by kind and by words in the name or what it asks', () => {
	const all = cards(board);
	assert.deepEqual(names(narrow(all, { show: 'locked', group: 'all', query: '' })), [
		'Tadpole Matinee',
		'Cinefrog',
		'Hidden Gem'
	]);
	assert.deepEqual(names(narrow(all, { show: 'all', group: 'moments', query: '' })), [
		'Triple Bill'
	]);
	assert.deepEqual(names(narrow(all, { show: 'all', group: 'all', query: 'films' })), [
		'Frogspawn',
		'Tadpole Matinee',
		'Cinefrog'
	]);
	assert.deepEqual(names(narrow(all, { show: 'all', group: 'all', query: 'ÁBOUT gem' })), [
		'Hidden Gem'
	]);
});

test('each order puts the right badge first and keeps ladders in order for ties', () => {
	const all = cards(board);
	assert.equal(sorted(all, 'recent')[0].name, 'Nami Lover');
	assert.equal(sorted(all, 'closest')[0].name, 'Tadpole Matinee');
	assert.equal(sorted(all, 'hardest')[0].name, 'Cinefrog');
	assert.deepEqual(names(sorted(all, 'easiest')).slice(0, 2), ['Frogspawn', 'Triple Bill']);
	assert.equal(sorted(all, 'rarest')[0].name, 'Tadpole Matinee');
	assert.equal(sorted(all, 'name')[0].name, 'Cinefrog');
});

test('by kind, the page is in sections in a fixed order, empty ones left out', () => {
	const sections = grouped(cards(board));
	assert.deepEqual(
		sections.map((s) => s.id),
		['watching', 'moments', 'taste', 'given']
	);
});

test('the summary picks the newest and the nearest', () => {
	const { newest, next } = highlights(cards(board));
	assert.equal(newest.name, 'Nami Lover');
	assert.equal(next.name, 'Tadpole Matinee');
});

test('how many hold it reads plainly', () => {
	assert.equal(heldBy(0, 3), 'Nobody here has it yet');
	assert.equal(heldBy(3, 3), 'Everyone here has it');
	assert.equal(heldBy(2, 3), 'Held by 2 of 3 people here');
});

test('the choices go into the address and come back out, unknown ones ignored', () => {
	const state = { show: 'earned', sort: 'hardest', group: 'taste', query: 'gem' };
	const text = toAddress(state);
	assert.equal(text, '?show=earned&sort=hardest&kind=taste&q=gem');
	assert.deepEqual(fromAddress(new URLSearchParams(text)), state);
	assert.equal(toAddress({ show: 'all', sort: 'kind', group: 'all', query: ' ' }), '');
	assert.deepEqual(fromAddress(new URLSearchParams('?show=nope&sort=up&kind=x')), {
		show: 'all',
		sort: 'kind',
		group: 'all',
		query: ''
	});
});
