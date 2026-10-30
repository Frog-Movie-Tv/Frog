/**
 * Turns RELEASE-NOTES.md into the JSON the "What's new" page renders, so the
 * changelog has exactly one source of truth and cannot drift from the app.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', '..', 'RELEASE-NOTES.md');
const target = join(here, '..', 'static', 'changelog.json');

/** The sections a release may have, in the order What's new shows them. */
export const SECTIONS = ['Highlights', 'Improvements', 'Fixes'];

/** Every release as What's new draws it, newest first. */
export function parseNotes(text) {
	const releases = [];

	let release = null;
	let group = null;
	let bullet = null;

	/** A bullet ends when the next one starts, not at the first line wrap. */
	const closeBullet = () => {
		if (bullet && group) group.items.push(bullet.trim());
		bullet = null;
	};

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

		// Screenshots are for the release page on GitHub. They live in docs/,
		// which is no part of the app, so they are not what What's new draws.
		if (/^!\[[^\]]*\]\(([^)]+)\)\s*$/.test(line)) {
			closeBullet();
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
	return releases.filter((r) => r.version.toLowerCase() !== 'unreleased');
}

/**
 * Everything that makes a release look unlike the others, in words. A drafted
 * section is waiting to be rewritten, so its one "Changes" list is let through.
 */
export function layoutProblems(text) {
	const problems = [];
	const drafted = new Set(
		[...text.matchAll(/^## \[([^\]]+)\].*\n+<!-- Drafted/gm)].map((m) => m[1])
	);
	for (const { version, codename, date, intro, groups } of parseNotes(text)) {
		const say = (what) => problems.push(`${version}: ${what}`);
		if (!codename) say('no codename');
		if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) say(`date "${date}" is not yyyy-mm-dd`);
		if (drafted.has(version)) continue;
		if (!intro) say('no sentence under the heading');
		const order = groups.map((g) => SECTIONS.indexOf(g.title));
		if (order.includes(-1)) say(`a section outside ${SECTIONS.join(', ')}`);
		if (order.some((n, i) => i > 0 && n <= order[i - 1])) say('sections out of order');
		if (groups.some((g) => !g.items.length)) say('an empty section');
		for (const item of groups.find((g) => g.title === 'Highlights')?.items ?? []) {
			if (!/^\*\*[^*]+[.?!]\*\* \S/.test(item)) say(`highlight without a bold lead: ${item}`);
		}
	}
	return problems;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
	const published = parseNotes(readFileSync(source, 'utf8'));
	writeFileSync(target, JSON.stringify(published, null, '\t') + '\n');
	console.log(`changelog.json: ${published.length} releases, newest ${published[0]?.version}`);
}
