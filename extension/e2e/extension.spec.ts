import {
	test as base,
	chromium,
	expect,
	type BrowserContext,
	type Frame,
	type Page,
	type Worker
} from '@playwright/test';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The extension's own API, inside `worker.evaluate`.
declare const chrome: any; // eslint-disable-line @typescript-eslint/no-explicit-any

const BUILD = join(dirname(fileURLToPath(import.meta.url)), '..', '.output', 'chrome-mv3');
const SITE = 'http://localhost:8072';
const FROG = 'http://127.0.0.1:8071';

interface Calls {
	resolve: Record<string, unknown>[];
	log: {
		title_id?: number;
		season?: number;
		episode?: number;
		watched_at: string;
		log_id: number;
	}[];
	undo: { log_id: number }[];
}

const frog = {
	calls: async (): Promise<Calls> => (await fetch(`${FROG}/__calls`)).json(),
	reset: () => fetch(`${FROG}/__reset`, { method: 'POST' }),
	fail: (fail: boolean) =>
		fetch(`${FROG}/__fail`, { method: 'POST', body: JSON.stringify({ fail }) })
};

const test = base.extend<{ context: BrowserContext; worker: Worker; extensionId: string }>({
	// eslint-disable-next-line no-empty-pattern
	context: async ({}, use) => {
		const context = await chromium.launchPersistentContext('', {
			// Playwright's own Chromium, which CI uses. `E2E_CHANNEL=msedge` runs it in
			// Edge on a machine where that build will not start.
			channel: process.env.E2E_CHANNEL ?? 'chromium',
			args: [
				`--disable-extensions-except=${BUILD}`,
				`--load-extension=${BUILD}`,
				'--autoplay-policy=no-user-gesture-required'
			]
		});
		await use(context);
		await context.close();
	},
	// Every test is connected, whether it reads the worker or not.
	worker: [
		async ({ context }, use) => {
			const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent('serviceworker'));
			await worker.evaluate(
				(url) =>
					chrome.storage.local.set({
						settings: { url, token: 'frog_test', threshold: 90, askFirst: false, ignored: [] }
					}),
				FROG
			);
			await use(worker);
		},
		{ auto: true }
	],
	extensionId: async ({ worker }, use) => {
		await use(new URL(worker.url()).host);
	}
});

test.beforeEach(async () => {
	await frog.reset();
});

/** Play the fixture's video, jump to `fraction` of it. */
async function play(where: Page | Frame, fraction?: number) {
	await where.waitForFunction(
		() => typeof (window as unknown as { playAt?: unknown }).playAt === 'function'
	);
	return where.evaluate(
		(f) => (window as unknown as { playAt: (f?: number) => Promise<unknown> }).playAt(f),
		fraction
	);
}

const toast = (page: Page) => page.locator('frog-toast .toast');

async function tabOf(worker: Worker, url: string): Promise<number> {
	return worker.evaluate(async (u) => (await chrome.tabs.query({ url: u }))[0]!.id!, url);
}

async function badge(worker: Worker, tabId: number) {
	return worker.evaluate((id) => chrome.action.getBadgeText({ tabId: id }), tabId);
}

test('ids in the page address log on their own, with Undo', async ({ context, worker }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/tv/95396/2/7`);
	await play(page);
	await expect.poll(async () => (await frog.calls()).resolve.length).toBeGreaterThan(0);
	const first = (await frog.calls()).resolve[0]!;
	expect(first).toMatchObject({ tmdb_id: 95396, kind: 'tv', season: 2, episode: 7 });

	await expect.poll(async () => badge(worker, await tabOf(worker, `${SITE}/*`))).toBe('S2E7');
	expect((await frog.calls()).log).toHaveLength(0);

	await play(page, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 2, episode: 7 });
	await expect(toast(page)).toContainText('Logged Severance S2E7');

	await toast(page).getByRole('button', { name: 'Undo' }).click();
	const logId = (await frog.calls()).log[0]!.log_id;
	await expect.poll(async () => (await frog.calls()).undo).toEqual([{ log_id: logId }]);
});

test('ids only in a cross-origin player’s address are found', async ({ context }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/iframe-only.html`);
	const player = page.frames().find((f) => f.url().startsWith('http://127.0.0.1:8072/embed'))!;
	await play(player, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 1, episode: 3 });
	expect((await frog.calls()).resolve.some((r) => r.tmdb_id === 95396)).toBe(true);
	await expect(toast(page)).toContainText('Logged Severance S1E3');
});

test('JSON-LD alone is enough', async ({ context }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/jsonld.html`);
	await play(page, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).resolve.at(-1)).toMatchObject({
		imdb_id: 'tt11280740',
		season: 1,
		episode: 2
	});
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 1, episode: 2 });
});

test('a title alone asks first, and the answer teaches the site', async ({ context, worker }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/title-only.html`);
	await play(page, 0.95);
	await expect(toast(page)).toContainText('Log Severance S1E3?');
	expect((await frog.calls()).log).toHaveLength(0);
	await toast(page).getByRole('button', { name: 'Log' }).click();
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);

	const rules = await worker.evaluate(async () => (await chrome.storage.local.get('rules')).rules);
	expect(rules).toMatchObject({
		localhost: { nameFrom: 'title', episodeFrom: 'title', titles: { severance: 1 } }
	});

	// Next time on this site the match is exact, and it logs on its own.
	await page.reload();
	await play(page, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(2);
	expect((await frog.calls()).resolve.at(-1)).toMatchObject({ title_id: 1 });
	await expect(toast(page)).toContainText('Logged Severance S1E3');
});

test('a video inside a shadow root is found', async ({ context }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/shadow.html?tmdb=95396&s=2&e=1`);
	await play(page, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 2, episode: 1 });
});

test('a pre-roll ad never counts, and the episode after it does, once', async ({ context }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/preroll.html?tmdb=95396&s=1&e=1`);
	const ad = (await play(page, 0.5)) as { duration: number };
	expect(ad.duration).toBeLessThan(180);
	// The ad runs out and hands the element to the episode.
	await page.waitForFunction(
		() =>
			(window as unknown as { video: () => HTMLVideoElement })
				.video()
				.currentSrc.endsWith('/media/episode.webm'),
		null,
		{
			timeout: 30_000
		}
	);
	await page.waitForTimeout(3000);
	expect((await frog.calls()).log, 'nothing while the ad played').toHaveLength(0);
	await play(page, 0.95);
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	await page.waitForTimeout(5000);
	expect((await frog.calls()).log).toHaveLength(1);
});

test('a page with nothing to go on asks, and the answer becomes the site’s rule', async ({
	context,
	worker,
	extensionId
}) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/nothing.html`);
	await play(page, 0.95);
	await expect(toast(page)).toContainText('What are you watching?');
	expect((await frog.calls()).log).toHaveLength(0);
	const tabId = await tabOf(worker, `${SITE}/*`);
	expect(await badge(worker, tabId)).toBe('?');

	const popup = await context.newPage();
	await popup.goto(`chrome-extension://${extensionId}/popup.html?tab=${tabId}`);
	await expect(popup.getByRole('heading', { name: 'What are you watching?' })).toBeVisible();
	await expect(popup.getByLabel('Search')).toHaveValue('Video');
	await popup.getByLabel('Search').fill('Severance');
	await popup.getByRole('button', { name: 'Search' }).click();
	await popup.getByRole('button', { name: /^Severance/ }).click();
	await popup.getByLabel('Season').fill('1');
	await popup.getByLabel('Episode').fill('4');
	await popup.getByRole('button', { name: 'Log Severance S1E4' }).click();
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 1, episode: 4 });

	const rules = await worker.evaluate(async () => (await chrome.storage.local.get('rules')).rules);
	expect(rules).toMatchObject({ localhost: { titles: { video: 1 } } });
});

test('a log Frog cannot take yet waits, and is never lost', async ({ context, extensionId }) => {
	const page = await context.newPage();
	await page.goto(`${SITE}/tv/95396/2/7`);
	await play(page);
	await expect.poll(async () => (await frog.calls()).resolve.length).toBeGreaterThan(0);
	await frog.fail(true);
	await play(page, 0.95);
	await expect(toast(page)).toContainText('reaches Frog as soon as it can');
	expect((await frog.calls()).log).toHaveLength(0);

	await frog.fail(false);
	const popup = await context.newPage();
	await popup.goto(`chrome-extension://${extensionId}/popup.html`);
	await expect(popup.getByText('One watch waiting to reach Frog')).toBeVisible();
	await popup.getByRole('button', { name: 'Send now' }).click();
	await expect.poll(async () => (await frog.calls()).log.length).toBe(1);
	expect((await frog.calls()).log[0]).toMatchObject({ title_id: 1, season: 2, episode: 7 });
});

test('an ignored site is left alone', async ({ context, worker }) => {
	await worker.evaluate(() =>
		chrome.storage.local
			.get('settings')
			.then(({ settings }: { settings: object }) =>
				chrome.storage.local.set({ settings: { ...settings, ignored: ['localhost'] } })
			)
	);
	const page = await context.newPage();
	await page.goto(`${SITE}/tv/95396/2/7`);
	await play(page, 0.95);
	await page.waitForTimeout(6000);
	const calls = await frog.calls();
	expect(calls.resolve).toHaveLength(0);
	expect(calls.log).toHaveLength(0);
});

test('nothing but what resolve needs ever reaches Frog', async ({ context }) => {
	for (const path of [
		'/tv/95396/2/7',
		'/iframe-only.html',
		'/jsonld.html',
		'/title-only.html',
		'/nothing.html'
	]) {
		const page = await context.newPage();
		await page.goto(`${SITE}${path}`);
		const player = page.frames().find((f) => f.url().includes('/embed')) ?? page;
		await play(player);
		await page.waitForTimeout(2500);
		await page.close();
	}
	const { resolve } = await frog.calls();
	expect(resolve.length).toBeGreaterThan(3);
	const allowed = [
		'title_id',
		'tmdb_id',
		'imdb_id',
		'kind',
		'season',
		'episode',
		'absolute',
		'title',
		'year',
		'page_title'
	];
	for (const body of resolve) {
		for (const key of Object.keys(body)) expect(allowed).toContain(key);
		expect(JSON.stringify(body)).not.toMatch(/https?:|localhost|127\.0\.0\.1|8072/);
	}
});
