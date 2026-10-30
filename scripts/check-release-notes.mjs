/**
 * Refuse a pull request that changes something people use and says nothing
 * about it.
 *
 *   node scripts/check-release-notes.mjs <base> <head>
 *
 * Notes are written with the change, under Upcoming in RELEASE-NOTES.md, and
 * the release takes them from there (draft-release-notes.mjs). Written later
 * they are a list of commit subjects, because nobody remembers what a change
 * was for by the time it ships.
 *
 * A commit that reaches nobody says so in its message: `Release-Notes: none`.
 */
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Conventional Commit types that reach people, unless the scope says otherwise. */
const REACHES_PEOPLE = /^(?:feat|fix)(?:\(([^)]*)\))?!?:/;
const BEHIND_THE_SCENES = new Set(['ci', 'build', 'deploy', 'test', 'tests', 'docs']);

/** Subjects of the commits that need a line in the notes. */
export function needingNotes(commits) {
	return commits
		.filter(({ subject, body }) => {
			const match = subject.match(REACHES_PEOPLE);
			if (!match) return false;
			if (BEHIND_THE_SCENES.has(match[1])) return false;
			return !/^Release-Notes:\s*none\s*$/im.test(body);
		})
		.map((c) => c.subject);
}

function commitsBetween(base, head) {
	const log = execFileSync('git', ['log', '--format=%s%x00%b%x01', `${base}..${head}`], {
		encoding: 'utf8'
	});
	return log
		.split('\u0001')
		.map((entry) => entry.trim())
		.filter(Boolean)
		.map((entry) => {
			const [subject, body = ''] = entry.split('\u0000');
			return { subject, body };
		});
}

function touchedNotes(base, head) {
	return execFileSync('git', ['diff', '--name-only', `${base}...${head}`], {
		encoding: 'utf8'
	})
		.split(/\r?\n/)
		.includes('RELEASE-NOTES.md');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
	const [base, head] = process.argv.slice(2);
	if (!base || !head) {
		console.error('usage: node scripts/check-release-notes.mjs <base> <head>');
		process.exit(2);
	}
	const waiting = needingNotes(commitsBetween(base, head));
	if (waiting.length && !touchedNotes(base, head)) {
		console.error('These reach people, and RELEASE-NOTES.md says nothing about them:');
		for (const subject of waiting) console.error(`  ${subject}`);
		console.error('');
		console.error('Add a line under Upcoming, in Improvements or Fixes, in product voice.');
		console.error('If a commit reaches nobody, add `Release-Notes: none` to its message.');
		process.exit(1);
	}
	console.log(waiting.length ? 'The notes cover this change.' : 'Nothing here needs a note.');
}
