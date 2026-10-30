// RELEASE-NOTES.md, read the way What's new reads it. Run with `npm test`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { layoutProblems, parseNotes } from '../scripts/changelog.mjs';

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
