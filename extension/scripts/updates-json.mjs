// The file Zen and Firefox read to find a newer version of the extension,
// written from the Firefox build's own manifest so its id and version can
// never disagree with what was signed.
//
//   node scripts/updates-json.mjs <address of the signed .xpi>  > updates.json

import { readFileSync } from 'node:fs';

const link = process.argv[2];
if (!link?.startsWith('https://')) {
	console.error('usage: node scripts/updates-json.mjs https://…/frog-extension-firefox.xpi');
	process.exit(1);
}
const manifest = JSON.parse(readFileSync('.output/firefox-mv3/manifest.json', 'utf8'));
const id = manifest.browser_specific_settings.gecko.id;
const update = { version: manifest.version, update_link: link };
process.stdout.write(JSON.stringify({ addons: { [id]: { updates: [update] } } }, null, 2) + '\n');
