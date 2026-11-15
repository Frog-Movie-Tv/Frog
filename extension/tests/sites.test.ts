import { test } from 'node:test';
import assert from 'node:assert/strict';
import { covers, switchSite, watching } from '../src/lib/sites.ts';

const everywhere = { allowOnly: false, allowed: [], ignored: ['youtube.com'] };
const allowlist = { allowOnly: true, allowed: ['max.com', 'netflix.com'], ignored: ['max.com'] };

test('a listed site covers its subdomains, and nothing that only ends like it', () => {
	assert.ok(covers(['max.com'], 'max.com'));
	assert.ok(covers(['max.com'], 'play.max.com'));
	assert.ok(!covers(['max.com'], 'hbomax.com'));
	assert.ok(!covers([], 'max.com'));
});

test('without the allowlist, every site but the ignored ones is watched', () => {
	assert.ok(watching(everywhere, 'netflix.com'));
	assert.ok(!watching(everywhere, 'youtube.com'));
	assert.ok(!watching(everywhere, 'm.youtube.com'));
	assert.ok(watching(everywhere, null));
});

test('with the allowlist, only the allowed sites are watched, whatever is ignored', () => {
	assert.ok(watching(allowlist, 'netflix.com'));
	assert.ok(watching(allowlist, 'play.max.com'), 'allowed outranks ignored');
	assert.ok(!watching(allowlist, 'youtube.com'));
	assert.ok(!watching(allowlist, null), 'a page with no site is not on the list');
});

test('the switch changes the list the mode uses, and ends where it was put', () => {
	assert.deepEqual(switchSite(everywhere, 'netflix.com', false), {
		ignored: ['netflix.com', 'youtube.com']
	});
	assert.deepEqual(switchSite(everywhere, 'm.youtube.com', true), { ignored: [] });
	assert.deepEqual(switchSite(allowlist, 'crunchyroll.com', true), {
		allowed: ['crunchyroll.com', 'max.com', 'netflix.com']
	});
	assert.deepEqual(switchSite(allowlist, 'play.max.com', false), { allowed: ['netflix.com'] });
	for (const [s, domain] of [
		[everywhere, 'm.youtube.com'],
		[allowlist, 'play.max.com'],
		[allowlist, 'crunchyroll.com']
	] as const) {
		for (const on of [true, false]) {
			const next = { ...s, ...switchSite(s, domain, on) };
			assert.equal(watching(next, domain), on, `${domain} switched ${on ? 'on' : 'off'}`);
		}
	}
});
