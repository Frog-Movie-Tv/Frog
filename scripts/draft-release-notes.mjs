/**
 * Make sure RELEASE-NOTES.md has a section for the version being released.
 *
 *   node scripts/draft-release-notes.mjs
 *
 * release-please writes CHANGELOG.md from the commit messages, but the What's
 * new page inside the app reads RELEASE-NOTES.md, which is written for people.
 * Those notes are written with each change, under Upcoming
 * (check-release-notes.mjs asks for them), so a release only has to give them
 * their version.
 *
 * With nothing under Upcoming, it drafts a section from the commit messages
 * and marks it as a draft, so the release is never blocked. Exits 0 and changes
 * nothing when the section already exists.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The placeholder under Upcoming while nothing is written. */
const WAITING = 'In progress.';

/** A sentence for a release whose notes do not open with one of their own. */
const PLAIN_INTRO = 'A few improvements and fixes.';

/**
 * A codename belongs to a big release, chosen by hand with `Release-As`. Small
 * releases carry the newest one, as 0.4.1 to 0.4.5 did.
 */
const newestCodename = (notes) => notes.match(/^## \[[\d.]+\] — ([^—]+) —/m)?.[1]?.trim() ?? 'Frog';

const release = (version) =>
	`[${version}]: https://github.com/Frog-Movie-Tv/Frog/releases/tag/v${version}`;

/** Put the link in with the others, or the heading links nowhere. */
const addLink = (notes, version) => notes.replace(/^(\[[\d.]+\]: .*)$/m, `${release(version)}\n$1`);

/**
 * Give what is written under Upcoming its version. Null when there is nothing
 * there to give: a heading with no bullets under it is not a note.
 */
export function promoteUpcoming(text, version, date) {
	const notes = text.replace(/\r\n/g, '\n');
	const top = notes.match(/^## Upcoming.*$/m);
	if (!top) return null;
	const start = top.index + top[0].length;
	const next = notes.slice(start).search(/^## \[/m);
	const end = next === -1 ? notes.length : start + next;
	const body = notes.slice(start, end).replace(WAITING, '').trim();
	if (!/^### .+\n+- /m.test(body)) return null;

	const first = body.search(/^### /m);
	const intro = body.slice(0, first).trim() || PLAIN_INTRO;
	const section = [
		`## [${version}] — ${newestCodename(notes)} — ${date}`,
		'',
		intro,
		'',
		body.slice(first).trim(),
		'',
		''
	].join('\n');
	const updated = `${notes.slice(0, start)}\n\n${WAITING}\n\n${section}${notes.slice(end)}`;
	return addLink(updated, version);
}

/** What release-please just wrote for this version, as its bullets. */
function bulletsFromChangelog(version) {
	let changelog;
	try {
		changelog = readFileSync(join(repo, 'CHANGELOG.md'), 'utf8');
	} catch {
		return [];
	}
	const lines = changelog.split(/\r?\n/);
	const start = lines.findIndex((l) => l.includes(`[${version}]`) || l.startsWith(`## ${version}`));
	if (start === -1) return [];

	const out = [];
	for (const line of lines.slice(start + 1)) {
		if (line.startsWith('## ')) break;
		const bullet = line.match(/^\s*\*\s+(.*)$/);
		if (!bullet) continue;
		const text = bullet[1]
			// Drop the scope, the commit links and the trailing references.
			.replace(/^\*\*[^:]+:\*\*\s*/, '')
			.replace(/\s*\(\[[0-9a-f]{6,}\]\([^)]*\)\)\s*$/, '')
			.replace(/\s*\(#\d+\)\s*$/, '')
			.trim();
		if (text) out.push(text.charAt(0).toUpperCase() + text.slice(1));
	}
	return out;
}

/** With nothing written: a draft from the commits, marked as one. */
function draftFromCommits(notes, version, date) {
	const bullets = bulletsFromChangelog(version);
	const section = [
		`## [${version}] — ${newestCodename(notes)} — ${date}`,
		'',
		'<!-- Drafted from the commit messages so the release is not blocked.',
		"     Rewrite it in Frog's voice before merging: say what a person can now",
		'     do, not what changed in the code. -->',
		'',
		'### Changes',
		'',
		...(bullets.length ? bullets.map((b) => `- ${b}`) : ['- Small fixes and improvements.']),
		''
	].join('\n');

	// Below the Upcoming heading, above the newest released version.
	const anchor = notes.search(/^## \[/m);
	const updated =
		anchor === -1
			? `${notes.trimEnd()}\n\n${section}`
			: `${notes.slice(0, anchor)}${section}\n${notes.slice(anchor)}`;
	return addLink(updated, version);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
	const notesPath = join(repo, 'RELEASE-NOTES.md');
	const version = readFileSync(join(repo, 'version.txt'), 'utf8').trim();
	const notes = readFileSync(notesPath, 'utf8');

	if (new RegExp(`^## \\[${version.replace(/\./g, '\\.')}\\]`, 'm').test(notes)) {
		console.log(`RELEASE-NOTES.md already has a section for ${version}.`);
		process.exit(0);
	}

	const date = new Date().toISOString().slice(0, 10);
	const written = promoteUpcoming(notes, version, date);
	writeFileSync(notesPath, written ?? draftFromCommits(notes, version, date), 'utf8');
	console.log(
		written
			? `The notes under Upcoming are now the ${version} section.`
			: `Nothing under Upcoming: drafted ${version} from the commit messages. Rewrite it before merging.`
	);
}
