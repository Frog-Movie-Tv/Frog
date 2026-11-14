import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	cleanName,
	cluesFrom,
	fromJsonLd,
	fromMediaSession,
	fromMeta,
	fromUrl,
	merge,
	nameFromTitle,
	namesAgree,
	numbersIn,
	type FrameReport
} from '../src/lib/signals.ts';

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

test('numbers are read the ways sites write them', () => {
	assert.deepEqual(numbersIn('Severance S02E07'), { season: 2, episode: 7 });
	assert.deepEqual(numbersIn('s2 e7'), { season: 2, episode: 7 });
	assert.deepEqual(numbersIn('The Bear 2x07'), { season: 2, episode: 7 });
	assert.deepEqual(numbersIn('Season 2 Episode 7'), { season: 2, episode: 7 });
	assert.deepEqual(numbersIn('Season 2 - Ep. 7'), { season: 2, episode: 7 });
	assert.deepEqual(numbersIn('Frieren Episode 28'), { season: undefined, episode: 28 });
	assert.deepEqual(numbersIn('Ep 12 | Mushishi'), { season: undefined, episode: 12 });
	assert.equal(numbersIn('Blade Runner 2049'), null);
	assert.equal(numbersIn('Sex Education'), null);
	assert.equal(numbersIn('1917'), null);
});

test('a name loses what pages wrap around it', () => {
	assert.equal(cleanName('Watch Severance S02E07 online free HD'), 'Severance');
	assert.equal(cleanName('The Bear (2022) Season 2 Episode 7'), 'The Bear');
	assert.equal(cleanName('Breaking.Bad.S01E01.720p'), 'Breaking Bad');
	assert.equal(nameFromTitle('Severance S2E7 | StreamSite'), 'Severance');
	assert.equal(nameFromTitle('Dune: Part Two - Watch on SomeSite'), 'Dune: Part Two');
});

test('names agree when one holds the other’s words', () => {
	assert.ok(namesAgree('Severance', 'Severance'));
	assert.ok(namesAgree('Pokémon Horizons', 'pokemon horizons: the series'));
	assert.ok(!namesAgree('Severance', 'The Bear'));
	assert.ok(!namesAgree('', 'The Bear'));
});

test('addresses give TMDB ids, IMDb ids and numbers', () => {
	assert.deepEqual(fromUrl('https://a.test/watch?tmdb=95396&s=2&e=7', 'page-url'), {
		source: 'page-url',
		tmdb: 95396,
		season: 2,
		episode: 7,
		kind: 'tv'
	});
	const shape = fromUrl('https://player.test/embed/tv/95396/1/3', 'iframe-src');
	assert.equal(shape?.tmdb, 95396);
	assert.equal(shape?.weak, true, 'an address shape alone is not proof');
	assert.deepEqual([shape?.season, shape?.episode, shape?.kind], [1, 3, 'tv']);
	const movie = fromUrl('https://a.test/movie/949-heat', 'page-url');
	assert.deepEqual([movie?.tmdb, movie?.kind], [949, 'movie']);
	assert.equal(fromUrl('https://a.test/title/tt0113277/', 'page-url')?.imdb, 'tt0113277');
	const slug = fromUrl('https://a.test/severance-season-2-episode-7', 'page-url');
	assert.deepEqual([slug?.season, slug?.episode], [2, 7]);
	assert.equal(fromUrl('https://a.test/about', 'page-url'), null);
	assert.equal(fromUrl('not a url', 'page-url'), null);
});

test('TMDB ids past 100,000 are read like any other', () => {
	const show = fromUrl('https://a.test/tv/247718/1/1?play=true', 'page-url');
	assert.deepEqual(
		[show?.tmdb, show?.kind, show?.season, show?.episode],
		[247718, 'tv', 1, 1],
		'a show from 2025'
	);
	const film = fromUrl('https://a.test/movie/1061474-superman', 'page-url');
	assert.deepEqual([film?.tmdb, film?.kind], [1061474, 'movie'], 'a film past a million');
	assert.equal(fromUrl('https://a.test/watch?tmdb=1061474', 'page-url')?.tmdb, 1061474);
	assert.equal(fromUrl('https://a.test/tmdb-1061474', 'page-url')?.tmdb, 1061474);
	// Numbers that size are never a season or an episode.
	assert.equal(fromUrl('https://a.test/watch?tmdb=95396&e=247718', 'page-url')?.episode, undefined);
});

test('JSON-LD names the episode, its show and their ids', () => {
	const [clue] = fromJsonLd([
		{
			'@context': 'https://schema.org',
			'@graph': [
				{
					'@type': 'TVEpisode',
					episodeNumber: 2,
					partOfSeason: { '@type': 'TVSeason', seasonNumber: 1 },
					partOfSeries: {
						'@type': 'TVSeries',
						name: 'Severance',
						sameAs: ['https://www.imdb.com/title/tt11280740/']
					}
				}
			]
		}
	]);
	assert.deepEqual(clue, {
		source: 'jsonld',
		kind: 'tv',
		episode: 2,
		season: 1,
		name: 'Severance',
		year: undefined,
		imdb: 'tt11280740'
	});
	const [film] = fromJsonLd([{ '@type': 'Movie', name: 'Heat', datePublished: '1995-12-15' }]);
	assert.deepEqual([film?.name, film?.year, film?.kind], ['Heat', 1995, 'movie']);
});

test('meta tags and the media session are read too', () => {
	const meta = fromMeta({ 'og:type': 'video.episode', 'og:title': 'Severance S1E3 | Site' });
	assert.deepEqual(
		[meta?.kind, meta?.season, meta?.episode, meta?.name],
		['tv', 1, 3, 'Severance']
	);
	const session = fromMediaSession({ title: 'S2:E7 Chikhai Bardo', album: 'Severance' });
	assert.deepEqual([session?.season, session?.episode, session?.name], [2, 7, 'Severance']);
});

test('a structured id beats text, and a plainly named id beats an address shape', () => {
	const clues = [
		...cluesFrom(page({ url: 'https://a.test/show/12/3/4', title: 'Severance S2E7' }), true),
		...cluesFrom(page({ url: 'https://player.test/e?tmdb=95396&s=2&e=7' }), false)
	];
	const { signals, from, weak } = merge(clues, 'Severance S2E7');
	assert.equal(signals.tmdb_id, 95396);
	assert.equal(from.id, 'frame-url');
	assert.equal(weak, false);
	assert.deepEqual([signals.season, signals.episode], [2, 7]);
	assert.equal(signals.title, 'Severance');
	assert.equal(signals.page_title, 'Severance S2E7');
});

test('ids in an iframe’s address count when the page has none', () => {
	const clues = cluesFrom(
		page({
			title: 'Now playing',
			iframes: ['https://player.test/embed?tmdb=95396&season=1&episode=3']
		}),
		true
	);
	const { signals, from } = merge(clues, 'Now playing');
	assert.deepEqual(
		[signals.tmdb_id, signals.season, signals.episode, from.id],
		[95396, 1, 3, 'iframe-src']
	);
});

test('an episode with no season is sent as counted from the start', () => {
	const { signals } = merge(
		cluesFrom(page({ title: 'Frieren Episode 28' }), true),
		'Frieren Episode 28'
	);
	assert.equal(signals.absolute, 28);
	assert.equal(signals.episode, undefined);
	assert.equal(signals.kind, 'tv');
});

test('only what resolve needs leaves: never an address', () => {
	const clues = cluesFrom(
		page({
			url: 'https://secret.test/watch/abc?token=xyz&tmdb=1',
			iframes: ['https://p.test/x?imdb=tt1234567']
		}),
		true
	);
	const { signals } = merge(clues, 'Something');
	const sent = JSON.stringify(signals);
	assert.ok(!sent.includes('secret.test') && !sent.includes('xyz') && !sent.includes('http'), sent);
	assert.deepEqual(
		Object.keys(signals).sort(),
		Object.keys(signals)
			.filter((k) =>
				[
					'title_id',
					'tmdb_id',
					'imdb_id',
					'kind',
					'season',
					'episode',
					'absolute',
					'title',
					'year',
					'page_title'
				].includes(k)
			)
			.sort()
	);
});

test('a site rule picks its source first and turns a learned name into Frog’s id', () => {
	const clues = cluesFrom(page({ title: 'Severance S1E3 | Site', headings: ['Episode 9'] }), true);
	const plain = merge(clues, 'x');
	assert.equal(plain.signals.title_id, undefined);
	const taught = merge(clues, 'x', {
		episodeFrom: 'title',
		nameFrom: 'title',
		titles: { severance: 1 }
	});
	assert.equal(taught.signals.title_id, 1);
	assert.deepEqual([taught.signals.season, taught.signals.episode], [1, 3]);
});
