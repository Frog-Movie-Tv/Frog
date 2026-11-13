// The two videos the end-to-end fixtures play: a four-minute "episode" and a
// thirty-second "ad". Tiny, silent, one frame a second, so they stay a few
// kilobytes and are kept in the repository; this only remakes them.
//
//   npm run videos        (needs ffmpeg on PATH)

import { execFileSync } from 'node:child_process';
import { mkdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'e2e', 'fixtures', 'media');
mkdirSync(out, { recursive: true });

for (const [name, seconds, colour] of [
	['episode', 240, '0x1b6b3a'],
	['ad', 30, '0xb03a2e']
]) {
	const file = join(out, `${name}.webm`);
	execFileSync('ffmpeg', [
		'-loglevel',
		'error',
		'-y',
		'-f',
		'lavfi',
		'-i',
		`color=c=${colour}:s=64x36:r=1:d=${seconds}`,
		'-c:v',
		'libvpx',
		'-b:v',
		'8k',
		'-g',
		'10',
		'-an',
		file
	]);
	console.log(`${file}  ${statSync(file).size} bytes`);
}
