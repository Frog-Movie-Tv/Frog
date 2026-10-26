/**
 * Turns RELEASE-NOTES.md into the JSON the "What's new" page renders, so the
 * changelog has exactly one source of truth and cannot drift from the app.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', '..', 'RELEASE-NOTES.md');
const target = join(here, '..', 'static', 'changelog.json');

const text = readFileSync(source, 'utf8');
const releases = [];

let release = null;
let group = null;
let bullet = null;

/** A bullet ends when the next one starts, not at the first line wrap. */
function closeBullet() {
	if (bullet && group) group.items.push(bullet.trim());
	bullet = null;
}

for (const line of text.split(/\r?\n/)) {
	const version = line.match(/^##\s+\[([^\]]+)\]\s*(?:—|-)?\s*(.*)$/);
	if (version) {
		closeBullet();
		// "0.3.0] — Tadpole — 2026-10-24": codename first, date last.
		const rest = version[2]
			.split(/\s+[—-]\s+/)
			.map((part) => part.trim())
			.filter(Boolean);
		const date = rest.length > 1 ? rest[rest.length - 1] : (rest[0] ?? '');
		const codename = rest.length > 1 ? rest.slice(0, -1).join(' ') : '';
		release = { version: version[1], codename, date, intro: '', groups: [] };
		releases.push(release);
		group = null;
		continue;
	}

	const heading = line.match(/^###\s+(.+)$/);
	if (heading && release) {
		closeBullet();
		group = { title: heading[1].trim(), items: [] };
		release.groups.push(group);
		continue;
	}

	const shot = line.match(/^!\[[^\]]*\]\(([^)]+)\)\s*$/);
	if (shot && release) {
		closeBullet();
		release.image = shot[1];
		continue;
	}

	const item = line.match(/^[-*]\s+(.*)$/);
	if (item && release) {
		closeBullet();
		if (!group) {
			group = { title: 'Changes', items: [] };
			release.groups.push(group);
		}
		bullet = item[1].trim();
		continue;
	}

	// An indented continuation belongs to the bullet above it.
	if (bullet !== null && /^\s+\S/.test(line)) {
		bullet += ' ' + line.trim();
		continue;
	}

	if (!line.trim()) {
		closeBullet();
		continue;
	}

	// Prose directly under a version heading is that release's summary.
	if (release && !group && !line.startsWith('[')) {
		release.intro = (release.intro ? release.intro + ' ' : '') + line.trim();
	}
}

closeBullet();

const published = releases.filter((r) => r.version.toLowerCase() !== 'unreleased');
writeFileSync(target, JSON.stringify(published, null, '\t') + '\n');
console.log(`changelog.json: ${published.length} releases, newest ${published[0]?.version}`);
