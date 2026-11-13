import { test } from 'node:test';
import assert from 'node:assert/strict';
import { backoff, due, label, outcome } from '../src/lib/queue.ts';
import { counts, pick, sittingKey, watched, type VideoSample } from '../src/lib/playback.ts';
import {
	checkRule,
	cleanDomain,
	describe,
	domainOf,
	learn,
	readExport,
	writeExport
} from '../src/lib/rules.ts';
import { cluesFrom, merge, type FrameReport } from '../src/lib/signals.ts';

const page = (over: Partial<FrameReport>): FrameReport => ({
	url: 'https://example.test/',
	iframes: [],
	jsonld: [],
	meta: {},
	mediaSession: null,
	title: '',
	headings: [],
	...over
});

test('a confirmed match teaches where the id, the episode and the name were', () => {
	const clues = cluesFrom(
		page({
			url: 'https://site.test/v/abc',
			iframes: ['https://p.test/embed/tv/95396/2/7'],
			title: 'Severance S2E7 | Site'
		}),
		true
	);
	const merged = merge(clues, 'Severance S2E7 | Site');
	const rule = learn(undefined, clues, merged, {
		title_id: 1,
		tmdb: { kind: 'tv', id: 95396 },
		season: 2,
		episode: 7
	});
	assert.equal(rule.idFrom, 'iframe-src');
	assert.equal(rule.episodeFrom, 'iframe-src');
	assert.equal(rule.nameFrom, 'title');
	assert.deepEqual(rule.titles, { severance: 1 });

	// Next time the weak address shape is trusted, because you said so.
	const again = merge(clues, 'x', rule);
	assert.equal(again.weak, false);
	assert.equal(again.signals.title_id, 1);
});

test('a correction teaches the name even with no id on the page', () => {
	const clues = cluesFrom(page({ title: 'Sevrance ep 3 - Site' }), true);
	const merged = merge(clues, 'x');
	const rule = learn(undefined, clues, merged, { title_id: 9, season: 1, episode: 3 });
	assert.equal(rule.idFrom, undefined);
	assert.equal(rule.episodeFrom, 'title');
	assert.deepEqual(rule.titles, { sevrance: 9 });
});

test('domains are the host without www, and anything else is refused', () => {
	assert.equal(domainOf('https://www.Example.test:8443/a?b'), 'example.test');
	assert.equal(domainOf('chrome://extensions'), null);
	assert.equal(cleanDomain(' https://www.new-site.example/path '), 'new-site.example');
	assert.equal(cleanDomain('localhost:8072'), 'localhost');
	assert.equal(cleanDomain('not a domain'), null);
	assert.equal(cleanDomain(''), null);
});

test('rules survive an export and refuse what is not a rule', () => {
	const rules = {
		'a.test': { idFrom: 'jsonld' as const, titles: { severance: 1 }, updated: '2026-11-13' }
	};
	assert.deepEqual(readExport(writeExport(rules)), { rules, skipped: 0 });
	const mixed = readExport(
		JSON.stringify({
			rules: {
				'b.test': { episodeFrom: 'title' },
				'c.test': { idFrom: 'document.querySelector' },
				'bad domain': { idFrom: 'jsonld' },
				'd.test': { titles: { x: -1 } }
			}
		})
	);
	assert.deepEqual(Object.keys(mixed.rules), ['b.test']);
	assert.equal(mixed.skipped, 3);
	assert.throws(() => readExport('nope'), /not a rules export/);
	assert.equal(checkRule([]), null);
	assert.match(
		describe({ idFrom: 'jsonld', titles: { a: 1 }, updated: '' }),
		/page’s data.*1 name/
	);
});

const video = (over: Partial<VideoSample>): VideoSample => ({
	id: 'a-1',
	src: 'blob:episode',
	duration: 2700,
	position: 0,
	playing: true,
	ended: false,
	area: 1000,
	fullscreen: false,
	...over
});

test('the biggest long video that plays is the one watched', () => {
	const ad = video({ id: 'a-1', duration: 30, area: 5000 });
	const small = video({ id: 'a-2', area: 100 });
	const main = video({ id: 'a-3', area: 900 });
	const paused = video({ id: 'a-4', area: 9000, playing: false });
	assert.equal(pick([ad, small, main, paused])?.id, 'a-3');
	assert.equal(pick([ad]), null, 'an ad under three minutes never counts');
	assert.equal(pick([video({ duration: Infinity })]), null, 'a live stream has no end to reach');
	assert.ok(!counts({ duration: 179 }) && counts({ duration: 180 }));
});

test('an ad handing over to the episode starts a new sitting', () => {
	const ad = sittingKey(video({ src: 'blob:x', duration: 200 }));
	const episode = sittingKey(video({ src: 'blob:x', duration: 2700 }));
	const swapped = sittingKey(video({ id: 'a-2', duration: 2700 }));
	assert.notEqual(ad, episode);
	assert.notEqual(episode, swapped);
	assert.equal(episode, sittingKey(video({ src: 'blob:x', duration: 2701 })));
});

test('it counts as watched past the threshold or at the end', () => {
	assert.ok(!watched(2400, 2700, false, 90));
	assert.ok(watched(2430, 2700, false, 90));
	assert.ok(watched(100, 2700, true, 90));
	assert.ok(watched(1350, 2700, false, 50));
	assert.ok(!watched(1300, 2700, false, 10), 'never below half, whatever is set');
	assert.ok(!watched(29, 30, true, 90), 'a clip never counts');
});

test('the queue keeps everything Frog has not taken', () => {
	assert.equal(outcome(200), 'done');
	assert.equal(outcome(0), 'retry');
	assert.equal(outcome(503), 'retry');
	assert.equal(outcome(429), 'retry');
	assert.equal(outcome(401), 'reconnect');
	assert.equal(outcome(404), 'refused');
	assert.equal(backoff(1), 60_000);
	assert.equal(backoff(20), 3_600_000);
	const q = [
		{ id: 'a', watched_at: '', label: '', tries: 0, next: 50 },
		{ id: 'b', watched_at: '', label: '', tries: 0, next: 10 },
		{ id: 'c', watched_at: '', label: '', tries: 0, next: 0, failed: 'no' },
		{ id: 'd', watched_at: '', label: '', tries: 0, next: 500 }
	];
	assert.deepEqual(
		due(q, 100).map((p) => p.id),
		['b', 'a']
	);
	assert.equal(label('Severance', 2, 7), 'Severance S2E7');
	assert.equal(label('Frieren', null, 28), 'Frieren episode 28');
	assert.equal(label('Heat'), 'Heat');
});
