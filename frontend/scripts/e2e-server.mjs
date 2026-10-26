/**
 * Boots the real Rust backend for Playwright against a throwaway database and
 * the freshly built frontend, so the end-to-end tests exercise the same binary
 * that ships in the Docker image.
 */
import { spawn } from 'node:child_process';
import { existsSync, statSync, rmSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');

const exe = process.platform === 'win32' ? 'frog.exe' : 'frog';
// Whichever build is newer, not whichever profile is listed first. Preferring
// release meant that changing the backend and running the suite without
// rebuilding release silently tested the previous version.
const binary = ['release', 'debug']
	.map((profile) => join(repo, 'backend', 'target', profile, exe))
	.filter(existsSync)
	.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];

if (!binary) {
	console.error(`No frog binary found. Build it first:\n  cd backend && cargo build --release`);
	process.exit(1);
}

// Each Playwright project gets its own port and its own database, so every
// project starts from a genuinely cold instance and the first-run setup test
// means the same thing in all of them.
const args = new Map();
for (const arg of process.argv.slice(2)) {
	const [k, v] = arg.replace(/^--/, '').split('=');
	args.set(k, v);
}
const port = args.get('port') ?? '8099';
const label = args.get('label') ?? 'default';

// A fresh database every run, so the first-run setup screen is always there.
const dataDir = join(repo, 'frontend', '.e2e-data', label);
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const child = spawn(binary, [], {
	stdio: 'inherit',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'e2e.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${port}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: '',
		FROG_ADMIN_PASSWORD: '',
		// Deliberately no TMDB key: the suite must not depend on the network.
		TMDB_API_KEY: '',
		RUST_LOG: 'frog=warn'
	}
});

const stop = () => child.kill();
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
child.on('exit', (code) => process.exit(code ?? 0));
