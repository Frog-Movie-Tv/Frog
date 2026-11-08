// The words of a card on Home. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { activityLine, alsoLine } from '../src/lib/activity.ts';

const item = (fields) => ({
	title: { id: 1, kind: 'tv', name: 'Dark', poster_path: null },
	did: 'watched',
	episodes: 0,
	season: null,
	episode: null,
	...fields
});

test('a finished show says so', () => {
	assert.equal(activityLine(item({ did: 'finished', episodes: 6 })), 'finished Dark');
});

test('a binge is a count', () => {
	assert.equal(activityLine(item({ episodes: 6 })), 'watched 6 episodes of Dark');
});

test('one episode names itself', () => {
	assert.equal(activityLine(item({ episodes: 1, season: 2, episode: 3 })), 'watched S2 E3 of Dark');
});

test('a film is just its name', () => {
	assert.equal(activityLine(item({ title: { name: 'Heat' } })), 'watched Heat');
	assert.equal(activityLine(item({ did: 'rewatched', title: { name: 'Heat' } })), 'rewatched Heat');
});

test('a rewatch keeps the count', () => {
	assert.equal(
		activityLine(item({ did: 'rewatched', episodes: 3 })),
		'rewatched 3 episodes of Dark'
	);
});

test('what got no card is counted', () => {
	assert.equal(alsoLine(1), 'and 1 other title');
	assert.equal(alsoLine(58), 'and 58 other titles');
});

test('a manga is read, by the chapter', () => {
	const berserk = { id: 2, kind: 'manga', name: 'Berserk', poster_path: null };
	assert.equal(activityLine(item({ title: berserk, episodes: 4 })), 'read 4 chapters of Berserk');
	assert.equal(
		activityLine(item({ title: berserk, episodes: 1, episode: 132 })),
		'read chapter 132 of Berserk'
	);
	assert.equal(activityLine(item({ title: berserk, did: 'finished' })), 'finished Berserk');
});
