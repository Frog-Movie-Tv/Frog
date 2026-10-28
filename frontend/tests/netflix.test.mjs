// The Netflix reader, against titles taken from a real 4,054-line export.
// Run with `npm test`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
	cleanTitle,
	netflixRows,
	readDays,
	readNetflix,
	searchName,
	settleNetflix
} from '../src/lib/import/netflix.ts';

const NOW = new Date('2026-10-28T12:00:00Z');
const fixture = readFileSync(
	new URL('./fixtures/netflix-viewing-history.csv', import.meta.url),
	'utf8'
);

function read(text = fixture) {
	return netflixRows(readNetflix(text, NOW), null, NOW);
}

/** label, kind, title, season, progress, status, last day, rewatch, year, other names, warning */
const row = (r) => [
	r.label,
	r.kind,
	r.title,
	r.season ?? null,
	r.progress ?? null,
	r.status,
	r.watched_at?.slice(0, 10) ?? null,
	r.rewatch,
	r.year,
	r.alt_titles ?? [],
	r.warning ?? null
];

test('the fixture comes out exactly as expected', () => {
	const { rows, skipped, notes } = read();
	assert.equal(skipped, 2);
	assert.deepEqual(notes, ['2 plays have no show name and were left out']);
	// One line per row reads better than prettier's ten.
	// prettier-ignore
	assert.deepEqual(rows.map(row), [
		["Cyberpunk: Edgerunners: Cyberpunk: Edgerunners 2","tv","Cyberpunk: Edgerunners",2,2,"watching","2026-10-23",false,null,["Cyberpunk"],null],
		["The Seven Deadly Sins: Four Knights of the Apocalypse: Season 2","tv","The Seven Deadly Sins: Four Knights of the Apocalypse",2,1,"watching","2026-10-20",false,null,["The Seven Deadly Sins"],null],
		["Monster: The Lizzie Borden Story: Bloodbath",null,"Monster: The Lizzie Borden Story: Bloodbath",null,1,"completed","2026-09-18",false,null,["Monster: The Lizzie Borden Story","Monster"],null],
		["Monster: The Lizzie Borden Story: Whack Job!",null,"Monster: The Lizzie Borden Story: Whack Job!",null,1,"completed","2026-09-17",false,null,["Monster: The Lizzie Borden Story","Monster"],null],
		["One Piece: Elbaph Arc","tv","One Piece",null,1,"watching","2026-09-14",false,null,[],"Choose which season this is"],
		["The Lord of the Rings: The Return of the King",null,"The Lord of the Rings: The Return of the King",null,1,"completed","2026-08-30",false,null,["The Lord of the Rings"],null],
		["The Lord of the Rings: The Two Towers",null,"The Lord of the Rings: The Two Towers",null,1,"completed","2026-08-29",false,null,["The Lord of the Rings"],null],
		["The Lord of the Rings: The Fellowship of the Ring",null,"The Lord of the Rings: The Fellowship of the Ring",null,1,"completed","2026-08-28",false,null,["The Lord of the Rings"],null],
		["Mission: Impossible - Fallout",null,"Mission: Impossible - Fallout",null,1,"completed","2026-08-02",false,null,["Mission"],null],
		["Mission: Impossible III",null,"Mission: Impossible III",null,1,"completed","2026-08-01",false,null,["Mission"],null],
		["Kingsman: The Golden Circle",null,"Kingsman: The Golden Circle",null,1,"completed","2026-07-19",false,null,["Kingsman"],null],
		["Kingsman: The Secret Service",null,"Kingsman: The Secret Service",null,1,"completed","2026-07-18",true,null,["Kingsman"],null],
		["Maze Runner: The Scorch Trials",null,"Maze Runner: The Scorch Trials",null,1,"completed","2026-07-04",false,null,["Maze Runner"],null],
		["The Hangover: Part II",null,"The Hangover: Part II",null,1,"completed","2026-06-21",false,null,["The Hangover"],null],
		["The Hangover",null,"The Hangover",null,null,"completed","2026-06-20",false,null,[],null],
		["Rebel Moon — Part Two: The Scargiver",null,"Rebel Moon — Part Two: The Scargiver",null,1,"completed","2026-06-13",false,null,["Rebel Moon — Part Two"],null],
		["Agneepath (2012)",null,"Agneepath",null,null,"completed","2026-06-06",false,2012,[],null],
		["Doctor (Telugu)",null,"Doctor",null,null,"completed","2026-05-30",false,null,[],null],
		["Game Over (Hindi Version)",null,"Game Over",null,null,"completed","2026-05-30",false,null,[],null],
		["Radhe Shyam (Hindi)",null,"Radhe Shyam",null,null,"completed","2026-05-23",false,null,[],null],
		["Inception",null,"Inception",null,null,"completed","2026-05-16",true,null,[],null],
		["Cyberpunk: Edgerunners: Lucky You",null,"Cyberpunk: Edgerunners: Lucky You",null,1,"completed","2026-04-25",false,null,["Cyberpunk: Edgerunners","Cyberpunk"],null],
		["Cyberpunk: Edgerunners: Let You Down",null,"Cyberpunk: Edgerunners: Let You Down",null,1,"completed","2026-04-25",false,null,["Cyberpunk: Edgerunners","Cyberpunk"],null],
		["VINLAND SAGA: Season 1","tv","VINLAND SAGA",1,2,"paused","2026-04-18",false,null,[],null],
		["One Piece: Egghead Arc ①","tv","One Piece",null,2,"watching","2026-04-11",false,null,[],"Choose which season this is"],
		["Squid Game: Season 3","tv","Squid Game",3,1,"paused","2026-03-28",false,null,[],null],
		["Squid Game: Season 2","tv","Squid Game",2,2,"paused","2026-03-27",false,null,[],null],
		["Rick and Morty: Season 3","tv","Rick and Morty",3,1,"paused","2026-03-14",false,null,[],null],
		["Beyblade X","tv","Beyblade X",null,2,"paused","2026-03-07",false,null,[],"Choose which season this is"],
		["Stranger Things: Stranger Things 3","tv","Stranger Things",3,1,"paused","2026-02-21",false,null,[],null],
		["Stranger Things","tv","Stranger Things",null,2,"paused","2026-02-14",false,null,[],"Choose which season this is"],
		["Beyblade Burst Turbo: Bonding! Aiger vs Valt!",null,"Beyblade Burst Turbo: Bonding! Aiger vs Valt!",null,1,"completed","2026-01-31",false,null,["Beyblade Burst Turbo"],null],
		["Demon Slayer: Kimetsu no Yaiba: Mugen Train Arc: Flame Hashira Kyojuro Rengoku",null,"Demon Slayer: Kimetsu no Yaiba: Mugen Train Arc: Flame Hashira Kyojuro Rengoku",null,1,"completed","2026-01-24",false,null,["Demon Slayer: Kimetsu no Yaiba: Mugen Train Arc","Demon Slayer: Kimetsu no Yaiba","Demon Slayer"],null],
		["Boruto: Naruto Next Generations: Academy Entrance: Boruto Uzumaki!",null,"Boruto: Naruto Next Generations: Academy Entrance: Boruto Uzumaki!",null,1,"completed","2026-01-17",false,null,["Boruto: Naruto Next Generations: Academy Entrance","Boruto: Naruto Next Generations","Boruto"],null],
		["The Eminence in Shadow: 1st Season","tv","The Eminence in Shadow",1,1,"paused","2026-01-10",false,null,[],null],
		["Hunter X Hunter (2011): Season 1","tv","Hunter X Hunter",1,1,"paused","2025-12-27",false,2011,[],null],
		["Crash Landing on You: Limited Series","tv","Crash Landing on You",1,2,"paused","2025-12-20",false,null,[],null],
		["Arnold: Limited Series","tv","Arnold",1,1,"paused","2025-12-13",false,null,[],null],
		["Avatar: The Last Airbender: Book 3","tv","Avatar: The Last Airbender",3,1,"paused","2025-12-06",false,null,["Avatar"],null],
		["BAKI: Season 1","tv","BAKI",1,2,"paused","2025-11-29",false,null,[],null],
		["KENGAN ASHURA: Season 2","tv","KENGAN ASHURA",2,1,"paused","2025-11-15",false,null,[],null],
		["KENGAN ASHURA: Part ll","tv","KENGAN ASHURA",null,1,"paused","2025-11-08",false,null,[],null],
		["KENGAN ASHURA: Part l","tv","KENGAN ASHURA",null,1,"paused","2025-11-01",false,null,[],null],
		["Money Heist: Part 1","tv","Money Heist",null,2,"paused","2025-10-25",false,null,[],null],
		["JoJo's Bizarre Adventure: STONE OCEAN","tv","JoJo's Bizarre Adventure",null,3,"paused","2025-10-18",false,null,[],"Choose which season this is"],
		["JoJo's Bizarre Adventure: Golden Wind","tv","JoJo's Bizarre Adventure",null,2,"paused","2025-10-11",false,null,[],"Choose which season this is"],
		["Bleach",null,"Bleach",null,null,"completed","2025-10-04",false,null,[],null],
		["Bleach: The Rescue","tv","Bleach",null,2,"paused","2025-09-27",false,null,[],"Choose which season this is"],
		["Bleach: The Entry","tv","Bleach",null,2,"paused","2025-09-26",false,null,[],"Choose which season this is"],
		["Pokémon the Series: Sun & Moon: Sun & Moon: Ultra Adventures","tv","Pokémon the Series: Sun & Moon",null,3,"paused","2025-09-20",false,null,["Pokémon the Series"],"Choose which season this is"],
		["Tokyo Ghoul: Tokyo Ghoul √A","tv","Tokyo Ghoul",null,3,"paused","2025-09-13",false,null,[],"Choose which season this is"],
		["Tokyo Ghoul: Ghoul",null,"Tokyo Ghoul: Ghoul",null,1,"completed","2025-09-06",false,null,["Tokyo Ghoul"],null],
		["Tokyo Ghoul: High Spirits",null,"Tokyo Ghoul: High Spirits",null,1,"completed","2025-09-06",false,null,["Tokyo Ghoul"],null],
		["Kakegurui: Kakegurui xx","tv","Kakegurui",null,3,"paused","2025-08-30",false,null,[],"Choose which season this is"],
		["Kakegurui","tv","Kakegurui",null,1,"paused","2025-08-23",false,null,[],"Choose which season this is"],
		["Death Note",null,"Death Note",null,null,"completed","2025-08-16",false,null,[],null],
		["DEATH NOTE: Death Note","tv","DEATH NOTE",null,2,"paused","2025-08-09",false,null,[],"Choose which season this is"],
		["Kuroko's Basketball: Last Game",null,"Kuroko's Basketball: Last Game",null,1,"completed","2025-08-02",false,null,["Kuroko's Basketball"],null],
		["Kuroko's Basketball: Season 1","tv","Kuroko's Basketball",1,1,"paused","2025-07-26",false,null,[],null],
		["Supergirl: Season 2","tv","Supergirl",2,1,"paused","2025-07-19",false,null,[],null],
		["Supergirl","tv","Supergirl",null,7,"paused","2025-07-12",false,null,[],"Choose which season this is"],
		["The Flash: Season 1","tv","The Flash",1,1,"paused","2025-06-21",false,null,[],null],
		["SHAMAN KING: Last Test: Shaman Fight",null,"SHAMAN KING: Last Test: Shaman Fight",null,1,"completed","2025-06-14",false,null,["SHAMAN KING: Last Test","SHAMAN KING"],null],
		["DAHMER: Monster: The Jeffrey Dahmer Story: Silenced",null,"DAHMER: Monster: The Jeffrey Dahmer Story: Silenced",null,1,"completed","2025-06-07",false,null,["DAHMER: Monster: The Jeffrey Dahmer Story","DAHMER: Monster","DAHMER"],null],
		["DAHMER: Monster: The Jeffrey Dahmer Story: Please Don't Go",null,"DAHMER: Monster: The Jeffrey Dahmer Story: Please Don't Go",null,1,"completed","2025-06-07",false,null,["DAHMER: Monster: The Jeffrey Dahmer Story","DAHMER: Monster","DAHMER"],null]
	]);
});

test('named seasons quote Netflix and say what they are', () => {
	const { rows } = read();
	const jojo = rows.find((r) => r.label === "JoJo's Bizarre Adventure: STONE OCEAN");
	assert.equal(jojo.info, 'Netflix calls this “STONE OCEAN”');
	assert.equal(jojo.season_name, 'STONE OCEAN');
	// "Heaven is at Hand: Three Days Until the New Moon" is one episode.
	assert.equal(jojo.progress, 3);

	const pokemon = rows.find((r) => r.title === 'Pokémon the Series: Sun & Moon');
	assert.equal(pokemon.season_name, 'Sun & Moon: Ultra Adventures');

	const part = rows.find((r) => r.label === 'KENGAN ASHURA: Part ll');
	assert.equal(part.info, 'Netflix calls this Part ll');
	assert.deepEqual(
		rows.find((r) => r.label === 'VINLAND SAGA: Season 1').netflix.numbers,
		[21, 22]
	);
});

test('films without a colon prefer a film, and nothing is rated', () => {
	const { rows } = read();
	for (const r of rows) assert.equal(r.rating, null);
	const singles = rows.filter((r) => r.netflix.single);
	assert.deepEqual(
		singles.map((r) => r.title),
		[
			'The Hangover',
			'Agneepath',
			'Doctor',
			'Game Over',
			'Radhe Shyam',
			'Inception',
			'Bleach',
			'Death Note'
		]
	);
	for (const r of singles) assert.equal(r.prefer, 'movie');
});

test('dates: month first, two-digit years, never in the future', () => {
	const us = readDays(['10/23/26', '1/5/22', '12/31/99'], NOW);
	assert.deepEqual(us.days, ['2026-10-23', '2022-01-05', '1999-12-31']);
	assert.equal(us.note, undefined);

	// A part over 12 in first place settles it for the whole file.
	const eu = readDays(['23/10/2026', '5/1/2022'], NOW);
	assert.deepEqual(eu.days, ['2026-10-23', '2022-01-05']);

	const unsure = readDays(['1/2/24', '3/4/24'], NOW);
	assert.deepEqual(unsure.days, ['2024-01-02', '2024-03-04']);
	assert.equal(unsure.note, 'Dates read as month, then day');

	assert.deepEqual(readDays(['2026-10-23', 'soon'], NOW).days, ['2026-10-23', null]);
});

test('titles are cleaned before anything else', () => {
	const bom = String.fromCharCode(0xfeff);
	assert.equal(cleanTitle(` Squid Game: Season 2: O ${bom} X `), 'Squid Game: Season 2: O X');
	assert.equal(cleanTitle('Vindicators 3:  The Return'), 'Vindicators 3: The Return');
	assert.deepEqual(searchName('Agneepath (2012)'), { name: 'Agneepath', year: 2012 });
	assert.deepEqual(searchName('Game Over (Hindi Version)'), { name: 'Game Over', year: null });
	assert.deepEqual(searchName('Hunter X Hunter (2011)'), { name: 'Hunter X Hunter', year: 2011 });
	assert.deepEqual(searchName('3%'), { name: '3%', year: null });
});

test('the data-request file: profiles, trailers and short plays', () => {
	const csv = [
		'Profile Name,Start Time,Duration,Attributes,Title,Supplemental Video Type,Device Type,Bookmark,Latest Bookmark,Country',
		'Zjef,2026-10-20 21:04:11,00:44:10,,The Flash: Season 1: Pilot (Episode 1),,TV,00:44:10,00:44:10,BE (Belgium)',
		'Zjef,2026-10-20 20:58:00,00:01:10,,The Flash: Season 1: Fastest Man Alive (Episode 2),,TV,00:01:10,00:01:10,BE (Belgium)',
		'Zjef,2026-10-19 20:00:00,00:02:30,,Season 1 Trailer: The Flash,TRAILER,TV,00:02:30,00:02:30,BE (Belgium)',
		'Mihai,2026-10-18 19:00:00,02:01:00,,Inception,,TV,02:01:00,02:01:00,BE (Belgium)',
		'Mihai,2026-10-17 19:00:00,00:00:20,,Arcane: Season 1: Welcome to the Playground,HOOK,TV,00:00:20,00:00:20,BE (Belgium)'
	].join('\n');
	const data = readNetflix(csv, NOW);
	assert.deepEqual(data.profiles, ['Mihai', 'Zjef']);
	assert.equal(data.ignored, 3);
	assert.deepEqual(
		data.plays.map((p) => [p.profile, p.day, p.title]),
		[
			['Zjef', '2026-10-20', 'The Flash: Season 1: Pilot'],
			['Mihai', '2026-10-18', 'Inception']
		]
	);

	const zjef = netflixRows(data, 'Zjef', NOW);
	assert.deepEqual(
		zjef.rows.map((r) => [r.label, r.season, r.progress]),
		[['The Flash: Season 1', 1, 1]]
	);
	assert.deepEqual(zjef.notes, ['3 trailers and short plays left out']);
});

test('a file that is not Netflix says so', () => {
	assert.throws(() => readNetflix('Name,Year\nHeat,1995\n', NOW), /not a Netflix viewing history/);
});

// ---------------------------------------------------------------- after matching

const show = (tmdb_id, name, seasons) => ({
	matched: { kind: 'tv', tmdb_id, name },
	seasons: seasons.map(([number, episodes, title]) => ({
		number,
		episodes,
		name: title ?? `Season ${number}`
	}))
});
const film = (tmdb_id, name, year = null) => ({
	matched: { kind: 'movie', tmdb_id, name, year },
	seasons: []
});

/** Fixture rows with stand-in TMDB answers, keyed by label. */
function settled(answers) {
	const rows = read().rows.map((incoming) => {
		const a = answers[incoming.label];
		const match = a
			? {
					key: incoming.key,
					matched: a.matched,
					candidates: [],
					note: a.note ?? null,
					already_tracked: null,
					seasons: a.seasons,
					searched: a.searched ?? null,
					placed: a.placed ?? []
				}
			: null;
		return { incoming, match, chosen: match?.matched ?? null, include: true };
	});
	return settleNetflix(rows, NOW);
}
const byLabel = (rows, label) => rows.find((r) => r.incoming.label === label)?.incoming;

test('titles matched one by one fold into their show', () => {
	const ghoul = show(61374, 'Tokyo Ghoul', [
		[1, 12, 'Tokyo Ghoul'],
		[2, 12, 'Tokyo Ghoul √A'],
		[3, 24, 'Tokyo Ghoul:re']
	]);
	const rows = settled({
		'Tokyo Ghoul: Tokyo Ghoul √A': ghoul,
		'Tokyo Ghoul: Ghoul': { ...ghoul, searched: 'Tokyo Ghoul' },
		'Tokyo Ghoul: High Spirits': { ...ghoul, searched: 'Tokyo Ghoul' },
		'Kingsman: The Secret Service': film(207703, 'Kingsman: The Secret Service'),
		'The Lord of the Rings: The Two Towers': film(121, 'The Lord of the Rings: The Two Towers')
	});

	const named = byLabel(rows, 'Tokyo Ghoul: Tokyo Ghoul √A');
	assert.equal(named.season, 2);
	assert.equal(named.warning, undefined);
	assert.equal(named.info, 'Netflix calls this “Tokyo Ghoul √A”; season 2 on TMDB');

	// The two unnamed episodes became one row under the show; the second is gone.
	const loose = byLabel(rows, 'Tokyo Ghoul');
	assert.equal(loose.title, 'Tokyo Ghoul');
	assert.equal(loose.progress, 2);
	assert.equal(loose.season, null);
	assert.equal(loose.warning, 'Choose which season this is');
	assert.equal(byLabel(rows, 'Tokyo Ghoul: High Spirits'), undefined);
	assert.equal(loose.status, 'paused');

	assert.equal(byLabel(rows, 'Kingsman: The Secret Service').rewatch, true);
	assert.equal(byLabel(rows, 'Kingsman: The Secret Service').status, 'completed');
});

test('seasons are placed by name, by count, or not at all', () => {
	const rows = settled({
		'Bleach: The Rescue': show(30984, 'Bleach', [
			[1, 20, 'The Substitute'],
			[2, 21, 'The Entry'],
			[3, 22, 'The Rescue']
		]),
		'DEATH NOTE: Death Note': show(13916, 'Death Note', [[1, 37]]),
		'Money Heist: Part 1': show(71446, 'Money Heist', [
			[1, 9, 'Part 1'],
			[2, 6, 'Part 2']
		]),
		'KENGAN ASHURA: Part l': show(86031, 'KENGAN ASHURA', [
			[1, 24],
			[2, 12]
		]),
		Supergirl: show(62688, 'Supergirl', [
			[1, 20],
			[2, 22]
		]),
		'Supergirl: Season 2': show(62688, 'Supergirl', [
			[1, 20],
			[2, 22]
		]),
		'VINLAND SAGA: Season 1': show(88803, 'VINLAND SAGA', [[1, 24]]),
		'One Piece: Egghead Arc ①': show(37854, 'One Piece', [
			[1, 61, 'East Blue'],
			[22, 40, 'Egghead Island']
		]),
		'Beyblade X': show(222000, 'BEYBLADE X', [[1, 51]]),
		'Beyblade Burst Turbo: Bonding! Aiger vs Valt!': show(67075, 'Beyblade Burst', [
			[1, 51, 'Beyblade Burst'],
			[2, 51, 'Beyblade Burst Evolution'],
			[3, 51, 'Beyblade Burst Turbo']
		]),
		'Monster: The Lizzie Borden Story: Bloodbath': {
			...show(9999, 'Monster', [[1, 74]]),
			searched: 'Monster'
		}
	});

	assert.equal(byLabel(rows, 'Bleach: The Rescue').season, 3);
	// One season on TMDB: nothing else it could be.
	assert.equal(byLabel(rows, 'DEATH NOTE: Death Note').season, 1);
	assert.equal(byLabel(rows, 'Money Heist: Part 1').season, 1);
	assert.equal(byLabel(rows, 'Money Heist: Part 1').warning, undefined);
	assert.equal(
		byLabel(rows, 'KENGAN ASHURA: Part l').warning,
		'Netflix calls this Part l; check the season'
	);

	const unnamed = byLabel(rows, 'Supergirl');
	assert.equal(unnamed.season, 1);
	assert.equal(unnamed.warning, 'Netflix gives no season here; counted as season 1');

	// "Episode 21" and "Episode 22": exactly those two.
	const vinland = byLabel(rows, 'VINLAND SAGA: Season 1');
	assert.equal(vinland.episode_offset, 20);
	assert.equal(vinland.progress, 2);

	const egghead = byLabel(rows, 'One Piece: Egghead Arc ①');
	assert.equal(egghead.season, 22);
	// TMDB numbers this season 1089 onwards too: exactly 1098 and 1099.
	assert.equal(egghead.episode_offset, 1097);
	assert.equal(egghead.warning, 'Netflix numbers these 1098–1099; check the episodes');

	assert.equal(byLabel(rows, 'Beyblade X').season, 1);
	assert.equal(byLabel(rows, 'Beyblade X').warning, undefined);

	// Netflix's show is one of TMDB's seasons.
	const turbo = byLabel(rows, 'Beyblade Burst Turbo: Bonding! Aiger vs Valt!');
	assert.equal(turbo.season, 3);
	assert.equal(turbo.warning, undefined);

	// Found only as "Monster", which is a different show by that name.
	const monster = byLabel(rows, 'Monster: The Lizzie Borden Story: Bloodbath');
	assert.equal(monster.season, 1);
	assert.equal(monster.warning, 'Found by searching “Monster”; check the match');
});

test('completed only when every season was seen to the end', () => {
	const squid = show(93405, 'Squid Game', [
		[1, 9],
		[2, 7],
		[3, 6]
	]);
	const partial = settled({ 'Squid Game: Season 2': squid, 'Squid Game: Season 3': squid });
	assert.equal(byLabel(partial, 'Squid Game: Season 2').status, 'paused');

	const whole = show(93405, 'Squid Game', [
		[2, 2],
		[3, 1]
	]);
	const done = settled({ 'Squid Game: Season 2': whole, 'Squid Game: Season 3': whole });
	assert.equal(byLabel(done, 'Squid Game: Season 2').status, 'completed');
	assert.equal(byLabel(done, 'Squid Game: Season 3').status, 'completed');
});

test('a film found only by a shorter name is flagged', () => {
	const rows = settled({
		'Rebel Moon — Part Two: The Scargiver': {
			...film(934632, 'Rebel Moon - Part Two: The Scargiver')
		},
		"Kuroko's Basketball: Last Game": {
			...film(1, "Kuroko's Basketball"),
			searched: "Kuroko's Basketball"
		}
	});
	assert.equal(byLabel(rows, 'Rebel Moon — Part Two: The Scargiver').warning, undefined);
	assert.equal(
		byLabel(rows, "Kuroko's Basketball: Last Game").warning,
		"Found by searching “Kuroko's Basketball”; check the match"
	);
});

test('a film released after it was watched is flagged', () => {
	const rows = settled({
		'Death Note': {
			...film(351460, 'Death Note', 2030),
			note: 'There is also a show by this name'
		},
		Bleach: { ...film(474395, 'Bleach', 2018), note: 'There is also a show by this name' }
	});
	const later = rows.find((r) => r.incoming.label === 'Death Note');
	assert.equal(later.incoming.warning, 'Released in 2030, after you watched it; check the match');
	// Out before it was watched: the film, and nothing to look at.
	const bleach = rows.find((r) => r.incoming.label === 'Bleach');
	assert.equal(bleach.incoming.warning, undefined);
	assert.equal(bleach.match.note, null);
});

test('episodes found by name settle the season, exactly and on their day', () => {
	const supergirl = read().rows.find((r) => r.label === 'Supergirl');
	// Episode names go along; the server looks them up only when in doubt.
	assert.deepEqual(supergirl.episodes, [
		'Better Angels',
		'Myriad',
		"World's Finest",
		'Falling',
		'Manhunter',
		'Childish Things',
		'Solitude'
	]);
	assert.deepEqual(read().rows.find((r) => r.label === 'Supergirl: Season 2').episodes, [
		'The Adventures of Supergirl'
	]);
	// "Episode 21" says nothing TMDB could look up by name.
	assert.equal(read().rows.find((r) => r.label === 'VINLAND SAGA: Season 1').episodes, undefined);

	const seasons = show(62688, 'Supergirl', [
		[1, 20],
		[2, 22]
	]);
	const rows = settled({
		Supergirl: {
			...seasons,
			// Five in season 1, one in season 2, "Solitude" nowhere.
			placed: [
				{ index: 0, season: 1, episode: 20 },
				{ index: 1, season: 1, episode: 19 },
				{ index: 2, season: 1, episode: 18 },
				{ index: 3, season: 1, episode: 16 },
				{ index: 4, season: 1, episode: 17 },
				{ index: 5, season: 2, episode: 3 }
			]
		},
		'Supergirl: Season 2': seasons
	});

	const first = byLabel(rows, 'Supergirl');
	assert.equal(first.season, 1);
	// "Solitude" was not found: counted next to the five that were, and
	// with no gap between them, just before the first.
	assert.equal(first.progress, 6);
	assert.deepEqual(first.watches.at(-1), { season: 1, episode: 15, watched_at: null });
	assert.deepEqual(first.watches[0], {
		season: 1,
		episode: 20,
		watched_at: '2025-07-12T20:00:00Z'
	});
	assert.equal(
		first.warning,
		'1 of 7 episodes not found by name; counted next to the ones that were'
	);
	assert.equal(first.info, 'Season 1, by its episode names');

	// The one from season 2 became a row of its own, right after.
	const at = rows.findIndex((r) => r.incoming === first);
	const second = rows[at + 1].incoming;
	assert.equal(second.key, `${first.key}-s2`);
	assert.equal(second.season, 2);
	assert.deepEqual(second.watches, [{ season: 2, episode: 3, watched_at: '2025-07-05T20:00:00Z' }]);
	assert.equal(second.warning, undefined);
});

test('a name that differs only in punctuation is not a problem', () => {
	const rows = settled({
		'Hunter X Hunter (2011): Season 1': {
			...show(46298, 'Hunter x Hunter', [[1, 148]]),
			note: 'Closest match by name'
		},
		'Doctor (Telugu)': { ...film(284052, 'Doctor Strange', 2016), note: 'Closest match by name' }
	});
	assert.equal(
		rows.find((r) => r.incoming.label === 'Hunter X Hunter (2011): Season 1').match.note,
		null
	);
	// Other words are another title: still for a person to look at.
	assert.equal(
		rows.find((r) => r.incoming.label === 'Doctor (Telugu)').match.note,
		'Closest match by name'
	);
});
