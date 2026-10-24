/**
 * Turns CHANGELOG.md into the JSON the "What's new" page renders, so the
 * changelog has exactly one source of truth and cannot drift from the app.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', '..', 'CHANGELOG.md');
const target = join(here, '..', 'static', 'changelog.json');

const text = readFileSync(source, 'utf8');
const releases = [];

let release = null;
let group = null;

for (const line of text.split(/\r?\n/)) {
	const version = line.match(/^##\s+\[([^\]]+)\]\s*(?:—|-)?\s*(.*)$/);
	if (version) {
		release = { version: version[1], date: version[2].trim(), intro: '', groups: [] };
		releases.push(release);
		group = null;
		continue;
	}

	const heading = line.match(/^###\s+(.+)$/);
	if (heading && release) {
		group = { title: heading[1].trim(), items: [] };
		release.groups.push(group);
		continue;
	}

	const item = line.match(/^[-*]\s+(.*)$/);
	if (item && release) {
		if (!group) {
			group = { title: 'Changes', items: [] };
			release.groups.push(group);
		}
		group.items.push(item[1].trim());
		continue;
	}

	// Prose directly under a version heading is that release's summary.
	if (release && !group && line.trim() && !line.startsWith('[')) {
		release.intro = (release.intro ? release.intro + ' ' : '') + line.trim();
	}
}

const published = releases.filter((r) => r.version.toLowerCase() !== 'unreleased');
writeFileSync(target, JSON.stringify(published, null, '\t') + '\n');
console.log(`changelog.json: ${published.length} releases, newest ${published[0]?.version}`);
