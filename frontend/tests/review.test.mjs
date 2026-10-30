// Sorting an import's rows for review. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	canKeepMatch,
	CLOSEST,
	groupRows,
	groupSummary,
	problemsOf
} from '../src/lib/import/review.ts';
import { CHOOSE } from '../src/lib/import/netflix.ts';

const tv = (tmdb_id, name = 'A Show') => ({ kind: 'tv', tmdb_id, name });

function row({ chosen = tv(1), note = null, warning, season = null, progress = null, key = 'k' }) {
	return {
		incoming: { key, title: 'x', warning, season, progress },
		match: { key, note },
		chosen,
		include: true
	};
}

test('every message lands on its kind of problem', () => {
	const kinds = (note, warning) => problemsOf(row({ note, warning }));

	assert.deepEqual(problemsOf(row({ chosen: null, note: 'No match found' })), ['missing']);
	assert.deepEqual(problemsOf(row({ chosen: null, note: 'TMDB did not answer' })), ['missing']);
	assert.deepEqual(kinds(null, undefined), []);
	assert.deepEqual(kinds(CLOSEST), ['closest']);

	for (const note of [
		'Your file says 1978, TMDB says 2008',
		'There is also a show by this name',
		'There is also a film by this name'
	]) {
		assert.deepEqual(kinds(note), ['match'], note);
	}
	for (const warning of [
		'Found by searching “Sing”; check the match',
		'Released in 2024, after you watched it; check the match'
	]) {
		assert.deepEqual(kinds(null, warning), ['match'], warning);
	}
	for (const warning of [
		CHOOSE,
		'Netflix gives no season here; counted as season 1',
		'Netflix calls this Part 2; check the season',
		"14 episodes watched, and TMDB's season 1 has 10; check the season"
	]) {
		assert.deepEqual(kinds(null, warning), ['season'], warning);
	}
	assert.deepEqual(kinds('TMDB lists no season 4 for this show; check the match'), ['season']);
	for (const warning of [
		'2 of 7 episodes not found by name; counted next to the ones that were',
		'Netflix numbers these 1089–1101; check the episodes',
		'Could not place this part; check the episodes',
		'Special: matched to the show, episodes not ticked'
	]) {
		assert.deepEqual(kinds(null, warning), ['episodes'], warning);
	}
});

test('a closest match with anything against it is not a closest match any more', () => {
	const released = 'Released in 2026, after you watched it; check the match';
	const zero = row({ note: CLOSEST, warning: released });
	assert.deepEqual(problemsOf(zero), ['match']);
	assert.equal(canKeepMatch(zero), false);

	const numbered = row({
		note: CLOSEST,
		warning: 'Netflix numbers these 1–76; check the episodes'
	});
	assert.deepEqual(problemsOf(numbered), ['match', 'episodes']);
	assert.equal(canKeepMatch(numbered), false);
});

test('only a plain closest match, still included, can be kept in bulk', () => {
	assert.equal(canKeepMatch(row({ note: CLOSEST })), true);
	assert.equal(canKeepMatch({ ...row({ note: CLOSEST }), include: false }), false);
	assert.equal(canKeepMatch(row({ note: null })), false);
	assert.equal(canKeepMatch(row({ note: 'Your file says 1978, TMDB says 2008' })), false);
	assert.equal(canKeepMatch(row({ warning: 'Choose which season this is' })), false);
	assert.equal(canKeepMatch(row({ chosen: null, note: CLOSEST })), false);
});

test('rows of one show come together, in season order, where the first one was', () => {
	const groups = groupRows([
		row({ key: 'a', chosen: tv(1), season: 3 }),
		row({ key: 'b', chosen: tv(2) }),
		row({ key: 'c', chosen: tv(1), season: 1 }),
		row({ key: 'd', chosen: { kind: 'movie', tmdb_id: 1, name: 'Same id, a film' } }),
		row({ key: 'e', chosen: null }),
		row({ key: 'f', chosen: null })
	]);
	assert.deepEqual(
		groups.map((g) => g.rows.map((r) => r.incoming.key)),
		[['c', 'a'], ['b'], ['d'], ['e'], ['f']]
	);
});

test('a show says how much is under it', () => {
	const seasons = [
		row({ season: 1, progress: 8 }),
		row({ season: 2, progress: 10 }),
		row({ season: 3, progress: 1 })
	];
	assert.equal(groupSummary(seasons), '3 seasons · 19 episodes');
	assert.equal(
		groupSummary([row({ season: 1, progress: 1 }), row({ season: null })]),
		'2 entries · 1 episode'
	);
	assert.equal(groupSummary([row({}), row({})]), '2 entries');
});
