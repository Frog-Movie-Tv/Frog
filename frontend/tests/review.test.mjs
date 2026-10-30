// Sorting an import's rows for review. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	canKeepMatch,
	CLOSEST,
	groupRows,
	groupSummary,
	holdBack,
	problemsOf,
	setHeld,
	SHOW_THRESHOLD
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

test('an anime only AniList has groups by its AniList id', () => {
	const anime = (anilist_id) => ({ kind: 'tv', tmdb_id: null, anilist_id, name: 'An Anime' });
	const groups = groupRows([
		row({ key: 'a', chosen: anime(21), season: 1 }),
		row({ key: 'b', chosen: anime(22), season: 1 }),
		row({ key: 'c', chosen: anime(21), season: 2 })
	]);
	assert.deepEqual(
		groups.map((g) => g.rows.map((r) => r.incoming.key)),
		[['a', 'c'], ['b']]
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

// ---------------------------------------------------------------- barely watched

/** One Netflix row of a show or film, as it stands after settling. */
function play({ id = 1, progress = 1, season = null, kind = 'tv', tracked = false, seconds, key }) {
	return {
		incoming: {
			key: key ?? `${kind}${id}-${season}`,
			title: 'x',
			season,
			progress,
			netflix: { seconds }
		},
		match: { already_tracked: tracked ? { status: 'watching' } : null },
		chosen: { kind, tmdb_id: id, name: 'A Title' },
		include: true
	};
}
const state = (rows) => rows.map((r) => [r.include, !!r.short]);

test('a show under the threshold is left out, one at it stays', () => {
	assert.equal(SHOW_THRESHOLD, 3);
	const rows = [
		play({ id: 1, progress: 1 }),
		play({ id: 2, progress: 2 }),
		play({ id: 3, progress: 3 }),
		play({ id: 4, progress: 40 })
	];
	holdBack(rows, SHOW_THRESHOLD);
	assert.deepEqual(state(rows), [
		[false, true],
		[false, true],
		[true, false],
		[true, false]
	]);
});

test('a show counts every season: one episode of a later one does not leave it out', () => {
	const rows = [
		play({ id: 1, season: 1, progress: 8 }),
		play({ id: 1, season: 2, progress: 1 }),
		play({ id: 2, season: 1, progress: 1 }),
		play({ id: 2, season: 2, progress: 1 })
	];
	holdBack(rows, 3);
	assert.deepEqual(
		rows.map((r) => r.include),
		[true, true, false, false]
	);
});

test('a title with no episode named reads as a film, whatever TMDB matched it to', () => {
	const rows = [play({ progress: 0 })];
	holdBack(rows, 3);
	assert.equal(rows[0].include, true);
	assert.equal(rows[0].short, false);
});

test('films stay when the file has no durations, and go only when it proves it', () => {
	const rows = [
		play({ kind: 'movie', id: 1, progress: null }),
		play({ kind: 'movie', id: 2, progress: null, seconds: 300 }),
		play({ kind: 'movie', id: 3, progress: null, seconds: 6000 })
	];
	holdBack(rows, 3);
	assert.deepEqual(
		rows.map((r) => r.include),
		[true, false, true]
	);
	holdBack(rows, 0);
	assert.equal(rows[1].include, true);
});

test('a show already in the library is never left out', () => {
	const rows = [
		play({ id: 1, season: 1, progress: 1, tracked: true }),
		play({ id: 1, season: 2, progress: 1 })
	];
	holdBack(rows, 3);
	assert.deepEqual(
		rows.map((r) => r.include),
		[true, true]
	);
});

test('other sources and rows with no match are left alone', () => {
	const other = {
		incoming: { key: 'o', title: 'x', progress: 1 },
		match: null,
		chosen: { kind: 'tv', tmdb_id: 9, name: 'Trakt' },
		include: true
	};
	const unmatched = { ...play({ id: 8 }), chosen: null };
	holdBack([other, unmatched], 3);
	assert.equal(other.include, true);
	assert.equal(unmatched.include, true);
	assert.equal(!!unmatched.short, false);
});

test('moving the threshold brings back what it left out, and only that', () => {
	const rows = [play({ id: 1, progress: 2 }), play({ id: 2, progress: 5 })];
	holdBack(rows, 3);
	assert.deepEqual(state(rows), [
		[false, true],
		[true, false]
	]);
	holdBack(rows, 6);
	assert.deepEqual(state(rows), [
		[false, true],
		[false, true]
	]);
	holdBack(rows, 2);
	assert.deepEqual(state(rows), [
		[true, false],
		[true, false]
	]);
	holdBack(rows, 0);
	assert.deepEqual(state(rows), [
		[true, false],
		[true, false]
	]);
});

test('what the person chose survives the threshold moving', () => {
	const added = play({ id: 1, progress: 1 });
	const dropped = play({ id: 2, progress: 9 });
	holdBack([added, dropped], 3);
	// Ticked back in by hand, and another taken out by hand.
	added.include = true;
	added.held = false;
	dropped.include = false;
	holdBack([added, dropped], 5);
	assert.equal(added.include, true, 'an added-back show stays');
	holdBack([added, dropped], 0);
	assert.equal(dropped.include, false, 'lowering never undoes a tick taken away by hand');
	holdBack([added, dropped], 3);
	assert.equal(dropped.include, false);
});

test('undoing "add back" leaves a show held, so the threshold can still bring it back', () => {
	const rows = [play({ id: 1, progress: 2 })];
	holdBack(rows, 3);
	setHeld(rows, false);
	assert.equal(rows[0].include, true);
	setHeld(rows, true);
	assert.equal(rows[0].include, false);
	holdBack(rows, 0);
	assert.equal(rows[0].include, true);
});

test('choosing another title counts that title instead', () => {
	const a = play({ id: 1, progress: 2 });
	const b = play({ id: 2, progress: 2, key: 'other' });
	holdBack([a, b], 3);
	assert.deepEqual(state([a, b]), [
		[false, true],
		[false, true]
	]);
	b.chosen = a.chosen;
	holdBack([a, b], 3);
	assert.deepEqual(state([a, b]), [
		[true, false],
		[true, false]
	]);
});
