// RELEASE-NOTES.md, read the way What's new reads it. Run with `npm test`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { layoutProblems, parseNotes } from '../scripts/changelog.mjs';
import { needingNotes } from '../../scripts/check-release-notes.mjs';
import { promoteUpcoming } from '../../scripts/draft-release-notes.mjs';

const notes = readFileSync(new URL('../../RELEASE-NOTES.md', import.meta.url), 'utf8');

test('every release in the notes follows the one layout', () => {
	assert.deepEqual(layoutProblems(notes), []);
});

test('a release reads as version, codename, date, a sentence and its sections', () => {
	const [release] = parseNotes(`## Upcoming — Bullfrog —

In progress.

## [1.2.3] — Toad — 2026-10-30

One sentence, wrapped
over two lines.

![A shot](docs/screenshots/home.png)

### Highlights

- **Search.** It finds things,
  even wrapped.

### Fixes

- A fix.

[1.2.3]: https://example.com
`);
	assert.deepEqual(release, {
		version: '1.2.3',
		codename: 'Toad',
		date: '2026-10-30',
		intro: 'One sentence, wrapped over two lines.',
		groups: [
			{ title: 'Highlights', items: ['**Search.** It finds things, even wrapped.'] },
			{ title: 'Fixes', items: ['A fix.'] }
		]
	});
});

test('anything that would look different is named', () => {
	const problems = (text) => layoutProblems(text).map((p) => p.replace(/^[\d.]+: /, ''));
	const ok = '## [1.0.0] — Toad — 2026-10-30\n\nA sentence.\n\n';

	assert.deepEqual(problems(`${ok}### Fixes\n\n- A fix.\n`), []);
	assert.deepEqual(problems('## [1.0.0] — 2026-10-30\n\nA sentence.\n'), ['no codename']);
	assert.deepEqual(problems('## [1.0.0] — Toad — soon\n\nA sentence.\n'), [
		'date "soon" is not yyyy-mm-dd'
	]);
	assert.deepEqual(problems('## [1.0.0] — Toad — 2026-10-30\n\n### Fixes\n\n- A fix.\n'), [
		'no sentence under the heading'
	]);
	assert.deepEqual(problems(`${ok}### Fixes\n\n- A.\n\n### Highlights\n\n- **B.** C.\n`), [
		'sections out of order'
	]);
	assert.deepEqual(problems(`${ok}### Changed\n\n- A.\n`), [
		'a section outside Highlights, Improvements, Fixes'
	]);
	assert.deepEqual(problems(`${ok}### Highlights\n\n- Plain.\n`), [
		'highlight without a bold lead: Plain.'
	]);
});

test('a drafted section waits to be rewritten without failing the release', () => {
	const draft = `## [1.0.0] — Toad — 2026-10-30

<!-- Drafted from the commit messages so the release is not blocked. -->

### Changes

- Small fixes and improvements.
`;
	assert.deepEqual(layoutProblems(draft), []);
});

const upcoming = (body) => `# Release notes

## Upcoming — Spring Peeper —

${body}

## [1.0.1] — Bullfrog — 2026-10-30

Earlier.

### Fixes

- An old fix.

[1.0.1]: https://github.com/Frog-Movie-Tv/Frog/releases/tag/v1.0.1
`;

test('what is under Upcoming becomes the release, in the one layout', () => {
	const notes = promoteUpcoming(
		upcoming('The library opens faster.\n\n### Fixes\n\n- Back keeps your place.'),
		'1.0.2',
		'2026-11-02'
	);
	assert.deepEqual(layoutProblems(notes), []);
	const [release, previous] = parseNotes(notes);
	assert.deepEqual(release, {
		version: '1.0.2',
		codename: 'Bullfrog',
		date: '2026-11-02',
		intro: 'The library opens faster.',
		groups: [{ title: 'Fixes', items: ['Back keeps your place.'] }]
	});
	assert.equal(previous.version, '1.0.1');
	assert.match(notes, /## Upcoming — Spring Peeper —\n\nIn progress\.\n\n## \[1\.0\.2\]/);
	assert.match(notes, /^\[1\.0\.2\]: .*\/tag\/v1\.0\.2$/m);
});

test('notes with no sentence of their own still open with one', () => {
	const notes = promoteUpcoming(upcoming('### Fixes\n\n- A fix.'), '1.0.2', '2026-11-02');
	assert.deepEqual(layoutProblems(notes), []);
	assert.equal(parseNotes(notes)[0].intro, 'A few improvements and fixes.');
});

test('a small feature carries the codename; only a big release takes a new one', () => {
	const notes = promoteUpcoming(upcoming('### Improvements\n\n- New.'), '1.1.0', '2026-11-02');
	assert.equal(parseNotes(notes)[0].codename, 'Bullfrog');
});

test('nothing written means nothing to promote', () => {
	assert.equal(promoteUpcoming(upcoming('In progress.'), '1.0.2', '2026-11-02'), null);
	assert.equal(promoteUpcoming(upcoming('### Fixes'), '1.0.2', '2026-11-02'), null);
	assert.equal(promoteUpcoming('# No heading\n', '1.0.2', '2026-11-02'), null);
});

test('promoting works on the real file, line endings and all', () => {
	const real = notes.replace(
		'In progress.',
		'A real sentence.\r\n\r\n### Fixes\r\n\r\n- A real fix.'
	);
	const after = promoteUpcoming(real, '1.0.2', '2026-11-02');
	assert.deepEqual(layoutProblems(after), []);
	assert.equal(parseNotes(after)[0].version, '1.0.2');
	assert.equal(parseNotes(after).length, parseNotes(notes).length + 1);
});

test('only what reaches people needs a note', () => {
	const c = (subject, body = '') => ({ subject, body });
	assert.deepEqual(
		needingNotes([
			c('fix(library): keep the scroll'),
			c('feat: a new page'),
			c('fix!: a breaking fix'),
			c('fix(ci): quote a path'),
			c('fix(deploy): wait for the volume'),
			c('chore: tidy'),
			c('docs: explain'),
			c('test(e2e): cover it'),
			c('refactor(store): move a query'),
			c('fix(search): nothing visible', 'Why it changed.\n\nRelease-Notes: none'),
			c('Merge pull request #1 from x/y')
		]),
		['fix(library): keep the scroll', 'feat: a new page', 'fix!: a breaking fix']
	);
});
