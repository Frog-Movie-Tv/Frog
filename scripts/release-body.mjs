/**
 * Print the written notes for one version, for the GitHub release page.
 *
 *   node scripts/release-body.mjs 0.5.0
 *
 * release-please writes the release page from CHANGELOG.md, which is a list of
 * commit subjects: fine for developers reading the repository, wrong for the
 * page anyone who follows a link lands on. RELEASE-NOTES.md is the one written
 * for people, so that is what goes on the release.
 *
 * If nobody has written that section, this prints nothing and the caller
 * leaves the generated body alone — a commit list reads better than an empty
 * page.
 */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const version = process.argv[2];
if (!version) {
	console.error('usage: node scripts/release-body.mjs <version>');
	process.exit(1);
}

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const notes = readFileSync(join(repo, 'RELEASE-NOTES.md'), 'utf8').split(/\r?\n/);

const heading = new RegExp(`^## \\[${version.replace(/\./g, '\\.')}\\]`);
const start = notes.findIndex((line) => heading.test(line));
if (start === -1) process.exit(0);

const body = [];
for (const line of notes.slice(start + 1)) {
	if (line.startsWith('## ')) break;
	body.push(line);
}

const text = body
	.join('\n')
	// The draft marker is a note to whoever writes the section, not to readers.
	.replace(/<!--[\s\S]*?-->/g, '')
	// Relative image paths do not resolve on a release page.
	.replace(/!\[([^\]]*)\]\((?!https?:)([^)]+)\)/g, (_, alt, path) => {
		const url = `https://raw.githubusercontent.com/Frog-Movie-Tv/Frog/v${version}/${path}`;
		return `![${alt}](${url})`;
	})
	.trim();

if (!text) process.exit(0);

process.stdout.write(`${text}\n`);
