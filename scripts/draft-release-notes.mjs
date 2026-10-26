/**
 * Make sure RELEASE-NOTES.md has a section for the version being released.
 *
 *   node scripts/draft-release-notes.mjs
 *
 * release-please writes CHANGELOG.md from the commit messages, but the What's
 * new page inside the app reads RELEASE-NOTES.md, which is written for people
 * rather than for developers. When nobody has written that section yet, the
 * release check fails and the release stops — which has now happened twice.
 *
 * So this drafts one from the same commit messages and marks it as a draft.
 * The release is never blocked, and the draft is there to be rewritten in
 * Frog's voice in the release pull request, which is the right place to do it.
 * Exits 0 and changes nothing when the section already exists.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const notesPath = join(repo, 'RELEASE-NOTES.md');
const version = readFileSync(join(repo, 'version.txt'), 'utf8').trim();
const notes = readFileSync(notesPath, 'utf8');

if (new RegExp(`^## \\[${version.replace(/\./g, '\\.')}\\]`, 'm').test(notes)) {
	console.log(`RELEASE-NOTES.md already has a section for ${version}.`);
	process.exit(0);
}

/** The codename carries across a patch, so reuse the newest one. */
const codename = notes.match(/^## \[[\d.]+\] — ([^—]+) —/m)?.[1]?.trim() ?? 'Lily Pad';

/** What release-please just wrote for this version, as its bullets. */
function bulletsFromChangelog() {
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

const bullets = bulletsFromChangelog();
const date = new Date().toISOString().slice(0, 10);

const section = [
	`## [${version}] — ${codename} — ${date}`,
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

const withLink = updated.replace(
	/^(\[[\d.]+\]: .*)$/m,
	`[${version}]: https://github.com/Frog-Movie-Tv/Frog/releases/tag/v${version}\n$1`
);

writeFileSync(notesPath, withLink, 'utf8');
console.log(`Drafted a ${version} section in RELEASE-NOTES.md from ${bullets.length} commits.`);
console.log('Rewrite it in product voice before merging the release.');
