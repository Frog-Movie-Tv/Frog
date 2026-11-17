/**
 * Draws every automatic badge from `backend/badges/catalogue.json` and writes
 * their SVGs to `backend/badges/art.json`, which the server turns into the
 * round PNGs a badge is stored as. Each event's two badges are drawn too,
 * into its pack (`backend/events/{slug}/ready.svg`, `opening.svg`), from
 * scenes under their keys (`event.{slug}.ready`). `--sheet <file>` also
 * writes a page with every badge on it, for looking them over.
 *
 *   npm run badges
 *   npm run badges -- --sheet ../badge-drafts/sheet.html
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { LADDER_RINGS, badge } from './badges/draw.mjs';
import { SCENES } from './badges/scenes.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const catalogue = JSON.parse(
	readFileSync(join(root, 'backend', 'badges', 'catalogue.json'), 'utf8')
);

/** Every badge with the ring it is drawn in and the number on its ribbon. */
export function entries() {
	const out = [];
	for (const ladder of catalogue.ladders) {
		let metal = 0;
		for (const step of ladder.steps) {
			const ring = step.at === 1 ? 'lily' : LADDER_RINGS[metal++];
			out.push({ ...step, ladder: ladder.id, ring, number: step.label ?? String(step.at) });
		}
	}
	for (const single of catalogue.singles) out.push({ ...single, number: null });
	for (const e of events()) out.push(e);
	return out;
}

/** Each event's badges, with the file each is written to. */
function events() {
	const dir = join(root, 'backend', 'events');
	if (!existsSync(dir)) return [];
	const out = [];
	for (const slug of readdirSync(dir)) {
		const file = join(dir, slug, 'event.json');
		if (!existsSync(file)) continue;
		const pack = JSON.parse(readFileSync(file, 'utf8'));
		const words = [
			['ready', pack.badges.ready, 'jade'],
			['opening', pack.badges.opening, 'gold']
		];
		for (const [which, w, ring] of words) {
			if (!w) continue;
			out.push({
				key: `event.${slug}.${which}`,
				name: w.name,
				about: w.about,
				kind: which === 'ready' ? 'total' : 'moment',
				ladder: 'events',
				ring,
				number: null,
				file: join(dir, slug, `${which}.svg`)
			});
		}
	}
	return out;
}

const art = {};
const missing = [];
for (const e of entries()) {
	const scene = SCENES[e.key];
	if (!scene) {
		missing.push(e.key);
		continue;
	}
	const svg = badge({ ring: e.ring, scene, number: e.number, note: `${e.name}: ${e.about}` });
	if (e.file) writeFileSync(e.file, svg + '\n');
	art[e.key] = svg;
}
if (missing.length) {
	console.error(`No scene for: ${missing.join(', ')}`);
	process.exit(1);
}
const extra = Object.keys(SCENES).filter((k) => !art[k]);
if (extra.length) {
	console.error(`Scenes for badges the catalogue does not have: ${extra.join(', ')}`);
	process.exit(1);
}

// The events' own are in their packs, not in the catalogue's file.
const ownArt = Object.fromEntries(
	entries()
		.filter((e) => !e.file)
		.map((e) => [e.key, art[e.key]])
);
writeFileSync(join(root, 'backend', 'badges', 'art.json'), JSON.stringify(ownArt, null, 0) + '\n');
console.log(`${Object.keys(art).length} badges drawn`);

const sheetAt = process.argv.indexOf('--sheet');
if (sheetAt > 0) {
	const file = process.argv[sheetAt + 1];
	const all = entries();
	const groups = [
		...new Set(all.map((e) => e.ladder ?? (e.kind === 'moment' ? 'moments' : 'totals')))
	];
	const cards = (g) =>
		all
			.filter((e) => (e.ladder ?? (e.kind === 'moment' ? 'moments' : 'totals')) === g)
			.map(
				(e) =>
					`<figure><img src="data:image/svg+xml;base64,${Buffer.from(art[e.key]).toString('base64')}" alt=""><figcaption><b>${e.name}</b><br>${e.about}</figcaption></figure>`
			)
			.join('');
	const html = `<!doctype html><meta charset="utf-8"><title>Badges</title>
<style>body{margin:0;font:14px system-ui;background:#0d1712;color:#e9efe9}h2{margin:24px 24px 8px;font-size:13px;text-transform:uppercase;letter-spacing:.08em;opacity:.6}
.row{display:flex;flex-wrap:wrap;gap:18px;padding:0 24px}figure{margin:0;width:150px;text-align:center}img{width:150px;height:150px}figcaption{font-size:12px;opacity:.85;margin-top:4px}
.small img{width:40px;height:40px}.small{display:flex;flex-wrap:wrap;gap:8px;padding:12px 24px}</style>
${groups.map((g) => `<h2>${g}</h2><div class="row">${cards(g)}</div>`).join('')}
<h2>Beside a name</h2><div class="small">${all.map((e) => `<img src="data:image/svg+xml;base64,${Buffer.from(art[e.key]).toString('base64')}" alt="">`).join('')}</div>`;
	writeFileSync(file, html);
	console.log(`sheet written to ${file}`);
}
