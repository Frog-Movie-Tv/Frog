import { expect, test, type Page } from '@playwright/test';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { enforceDeployedCsp } from '../scripts/caddy-csp.mjs';

/**
 * These run against the real Rust binary with an empty database and no TMDB
 * key, so they cover the paths that must work on a cold self-hosted install:
 * first-run setup, the auth wall, the shell, and the degraded-but-clear state
 * when TMDB is not configured yet.
 */

const USER = 'mihai';
const PASS = 'a-long-enough-pass';

// The database is created once per run, so the very first test does setup and
// the rest reuse the session via the browser context.
test.describe.configure({ mode: 'serial' });

// Every page is served with the policy Caddy sends the live site, read from
// deploy/caddy/Caddyfile, and anything it blocks fails the test. Nothing else
// local ever sees that header, and it has broken the live site three times.
let blocked: string[] = [];
test.beforeEach(async ({ context, baseURL }) => {
	blocked = await enforceDeployedCsp(context, baseURL ?? '');
});
test.afterEach(() => {
	expect(blocked, 'blocked by the deployed Content-Security-Policy').toEqual([]);
});

// Retrying this one is pointless: the suite is serial and shares one
// instance, so by the second attempt the account exists and the instance
// is no longer cold.
test.describe('a cold instance', () => {
	test.describe.configure({ retries: 0 });

	test('a cold instance sends you to first-run setup', async ({ page }) => {
		await page.goto('/');
		await expect(page).toHaveURL(/\/login/);
		// The heading only settles once the shell has asked whether this instance
		// has an account, and the very first request hits a server that has just
		// started and migrated.
		// CI's runners are slow to start the very first time; 15 seconds was flaky.
		await expect(page.getByRole('heading', { name: 'Set up Frog' })).toBeVisible({
			timeout: 30000
		});
	});
});

test('creating the account signs you straight in', async ({ page }) => {
	await page.goto('/login');

	const heading = page.getByRole('heading');
	await expect(heading).toBeVisible();

	// Either this worker is first (setup) or another already created it (login).
	const isSetup = (await heading.textContent())?.includes('Set up');

	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page.getByRole('button', { name: isSetup ? 'Create account' : 'Sign in' }).click();

	// A new account with nothing tracked is welcomed first; on a retry the
	// account may have been welcomed already and Home stays. Home shows its
	// empty state only once it knows which, so whichever of the two appears
	// is the answer. A fixed wait for the welcome alone raced slow runners.
	const welcome = page.getByRole('heading', { name: /^Welcome/ });
	const empty = page.getByText('Nothing here yet');
	await expect(welcome.or(empty)).toBeVisible({ timeout: 30000 });
	if (await welcome.isVisible()) {
		await page.getByRole('button', { name: 'Continue' }).click();
		await expect(page.getByRole('heading', { name: 'Fill your library' })).toBeVisible();
		await page.getByRole('button', { name: 'Later' }).click();
		// Leaving at once, as closing the app would: being welcomed has to be
		// saved before the page goes, or the next sign-in welcomes again.
		await expect(page).toHaveURL('/');
		await page.close();
		return;
	}
	await expect(page).toHaveURL('/');
	await expect(page.getByText('Nothing here yet')).toBeVisible();
});

test('once welcomed, an empty home stays home', async ({ page }) => {
	await signIn(page);
	await page.goto('/');
	await expect(page.getByText('Nothing here yet')).toBeVisible();
	await page.waitForTimeout(500);
	await expect(page).toHaveURL('/');
});

test('a server that cannot answer for a moment does not sign you out', async ({ page }) => {
	await signIn(page);
	// What the proxy answers while Frog restarts for an update.
	await page.route('**/api/auth/status', (route) => route.fulfill({ status: 502, body: '' }));
	await page.goto('/library');
	await page.waitForLoadState('networkidle');
	await expect(page).toHaveURL(/\/library$/);
	await expect(page.getByRole('heading', { name: 'Library', level: 1 })).toBeVisible();
	await page.unroute('**/api/auth/status');
});

test('an empty home and library explain themselves instead of showing blank grids', async ({
	page
}) => {
	await signIn(page);
	await expect(page.getByText('Nothing here yet')).toBeVisible();

	await page.goto('/library');
	await expect(page.getByRole('heading', { name: 'Library', level: 1 })).toBeVisible();
	await expect(page.getByText('Your library is empty')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Find something to watch' })).toBeVisible();
});

test('every page names itself in the browser tab', async ({ page }) => {
	await signIn(page);

	for (const [path, expected] of [
		['/', /^Frog$/],
		['/library', /^Library · Frog$/],
		['/search', /^Search · Frog$/],
		['/calendar', /^Calendar · Frog$/],
		['/airing', /^Airing · Frog$/],
		['/diary', /^Diary · Frog$/],
		['/calendar?view=diary', /^Diary · Frog$/],
		['/stats', /^Stats · Frog$/],
		['/settings', /^Settings · Frog$/],
		['/about', /^About · Frog$/],
		['/import', /^Import · Frog$/],
		['/whats-new', /^What's new · Frog$/]
	] as const) {
		await page.goto(path);
		await expect(page).toHaveTitle(expected);
	}
});

test('an unknown address says so and leads home', async ({ page }) => {
	await signIn(page);
	await page.goto('/no-such-page');

	await expect(page).toHaveTitle(/^Not found · Frog$/);
	await expect(page.getByRole('heading', { name: 'Nothing here' })).toBeVisible();
	await page.getByRole('link', { name: 'Go home' }).click();
	await expect(page).toHaveURL(/\/$/);
});

test('a bad title or shelf address reads like Frog, not like the server', async ({ page }) => {
	await signIn(page);

	for (const [path, heading] of [
		['/title/book/1', "That isn't a film or a show"],
		['/title/movie/abc', "This title isn't on TMDB"],
		['/t/abc', "This title isn't on TMDB"],
		['/shelves/99999', 'No shelf here']
	] as const) {
		await page.goto(path);
		await expect(page.getByRole('heading', { name: heading })).toBeVisible();
		await expect(page).toHaveTitle(/^Not found · Frog$/);
		await expect(page.getByText(/must be|Request failed|no such/i)).toHaveCount(0);
	}
});

test('the stats page loads instead of showing a database error', async ({ page }) => {
	await signIn(page);
	await page.goto('/stats');

	await expect(page.getByRole('heading', { name: 'Stats', level: 1 })).toBeVisible();
	// A fresh instance has watched nothing: it says so rather than drawing zeros.
	await expect(page.getByText('Nothing watched yet')).toBeVisible();
	// A year with nothing in it is an empty state too, not an error.
	await page.goto('/stats?year=2020');
	await expect(page.getByText('Nothing watched yet')).toBeVisible();
	// The exact failure that shipped in 0.1.0.
	await expect(page.getByText(/mismatched types/i)).toHaveCount(0);
	await expect(page.getByText(/Rust type/i)).toHaveCount(0);
});

test('the command palette opens and can navigate', async ({ page }) => {
	await signIn(page);

	await page.keyboard.press('Control+k');
	const field = page.getByPlaceholder('Jump to a title, a page or an action');
	await expect(field).toBeVisible();

	await field.fill('library');
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL('/library');
});

test('the command palette input sits on one line with its icon and esc badge', async ({
	page
}, testInfo) => {
	await signIn(page);
	await page.keyboard.press('Control+k');

	const dialog = page.getByRole('dialog', { name: 'Command palette' });
	const input = dialog.getByPlaceholder('Jump to a title, a page or an action');
	await expect(input).toBeVisible();

	const row = dialog.locator('.query');
	const icon = row.locator('svg').first();
	// A phone has a close button where a keyboard has its esc badge.
	const esc = row.locator('kbd:visible, button:visible');

	const [inputBox, iconBox, escBox, rowBox] = await Promise.all([
		input.boundingBox(),
		icon.boundingBox(),
		esc.boundingBox(),
		row.boundingBox()
	]);
	const middle = (b: { y: number; height: number } | null) => (b ? b.y + b.height / 2 : NaN);

	// All three share one horizontal line, in order, and the row stays one line tall.
	expect(Math.abs(middle(iconBox) - middle(inputBox))).toBeLessThan(6);
	expect(Math.abs(middle(escBox) - middle(inputBox))).toBeLessThan(6);
	expect(iconBox!.x).toBeLessThan(inputBox!.x);
	expect(escBox!.x).toBeGreaterThan(inputBox!.x);
	expect(rowBox!.height).toBeLessThan(64);

	await page.screenshot({ path: testInfo.outputPath('palette.png') });
});

test('keyboard hints only show where there is a keyboard', async ({ page, isMobile }) => {
	await signIn(page);
	await page.goto('/');
	const palette = page.getByRole('dialog', { name: 'Command palette' });
	await page.getByRole('button', { name: 'Search and commands' }).click();
	await expect(palette).toBeVisible();

	const hints = palette.locator('kbd:visible');
	const close = palette.getByRole('button', { name: 'Close' });
	if (isMobile) {
		await expect(page.locator('.bar kbd')).toBeHidden();
		await expect(hints).toHaveCount(0);
		await close.click();
		await expect(palette).toBeHidden();
	} else {
		// A Mac writes it ⌘K, everything else Ctrl K.
		await expect(page.locator('.bar kbd')).toHaveText(/^(Ctrl K|⌘K)$/);
		await expect(palette.getByText('esc')).toBeVisible();
		await expect(palette.locator('footer')).toBeVisible();
		await expect(close).toBeHidden();
		await page.keyboard.press('Escape');
		await expect(palette).toBeHidden();
	}
});

test("the what's new page lists the current release", async ({ page }) => {
	await signIn(page);
	await page.goto('/whats-new');

	await expect(page.getByRole('heading', { name: "What's new" })).toBeVisible();
	await expect(page.getByText('Installed', { exact: true })).toBeVisible();
	await expect(page.getByText('Ribbit').first()).toBeVisible();

	// Release notes must not be cut off at the first line wrap, and <kbd>
	// must render as a key rather than as literal angle brackets.
	const bullet = page
		.locator('li')
		.filter({ hasText: /Import from Letterboxd, Trakt, IMDb/ })
		.first();
	await expect(bullet).toContainText('you decide whether the file or your library wins');

	const shortcut = page
		.locator('li')
		.filter({ hasText: /command palette/i, has: page.locator('kbd') })
		.first();
	await expect(shortcut.locator('kbd').first()).toBeVisible();
	await expect(page.getByText('<kbd>')).toHaveCount(0);
});

test('the importer offers every supported source and both exports', async ({ page }) => {
	await signIn(page);
	await page.goto('/import');

	for (const source of ['MyAnimeList', 'Letterboxd', 'IMDb', 'Trakt', 'TV Time', 'Frog backup']) {
		await expect(page.getByRole('button', { name: source, exact: true })).toBeVisible();
	}

	// AniList is not a file: it is connected from Settings.
	await expect(page.getByRole('link', { name: 'Settings' }).last()).toBeVisible();
	await expect(page.getByRole('link', { name: 'Download backup' })).toBeVisible();
	await expect(page.getByRole('link', { name: 'Download CSV' })).toBeVisible();
});

test('calendar and diary load and explain themselves when empty', async ({ page }) => {
	await signIn(page);

	await page.goto('/calendar');
	await expect(page.getByRole('heading', { name: 'Calendar', level: 1 })).toBeVisible();
	await expect(page.getByText('Nothing scheduled')).toBeVisible();

	await page.goto('/diary');
	await expect(page).toHaveURL(/\/calendar\?view=diary$/);
	await expect(page.getByRole('button', { name: 'Diary', pressed: true })).toBeVisible();
	await expect(page.getByText('Nothing watched yet')).toBeVisible();
});

test('the about page credits its sources', async ({ page }) => {
	await signIn(page);
	await page.goto('/about');

	await expect(page.getByRole('heading', { name: 'Frog', level: 1 })).toBeVisible();
	await expect(page.getByText('Chorus Frog')).toBeVisible();
	await expect(page.getByRole('link', { name: /TMDB/ }).first()).toBeVisible();
	await expect(page.getByRole('link', { name: /Phosphor/ })).toBeVisible();
});

test('the instance asks not to be indexed', async ({ request }) => {
	const robots = await request.get('/robots.txt');
	expect(robots.ok()).toBeTruthy();
	expect(await robots.text()).toContain('Disallow: /');

	const page = await request.get('/api/health');
	expect(page.headers()['x-robots-tag']).toContain('noindex');
});

test('search says plainly when metadata is not connected', async ({ page }) => {
	await signIn(page);
	await page.goto('/search');
	await expect(page.getByPlaceholder('Films, shows, people, characters, studios')).toBeVisible();
	await expect(page.getByText('Search is unavailable until metadata is connected')).toBeVisible();
});

test('settings shows the instance facts and switches theme', async ({ page }) => {
	await signIn(page);
	// Settings comes in groups; each has its own address.
	await page.goto('/settings#data');

	// The name shows in Account too; the facts live in This instance.
	const facts = page.locator('#this-instance');
	await expect(facts.getByText(USER)).toBeVisible();
	await expect(facts.getByText('missing')).toBeVisible();

	await page
		.getByRole('navigation', { name: 'Settings' })
		.getByRole('link', { name: 'Appearance' })
		.click();

	// Glass is the default.
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'glass');

	await page.getByRole('button', { name: 'Dark', exact: true }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

	// The choice must survive a reload, or it is not a setting.
	await page.reload();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

	await page.getByRole('button', { name: 'Glass Light', exact: true }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'glass-light');

	// A glow of its own lights the page on any theme, and goes with the theme's own.
	await page.getByRole('radio', { name: 'Green' }).click();
	await expect(page.locator('html')).toHaveAttribute('style', /--room-1: rgba\(80, 210, 110/);
	await page.getByRole('radio', { name: 'The theme’s own' }).click();
	await expect(page.locator('html')).not.toHaveAttribute('style', /--room-1/);
});

test('preferences are kept on the server, not just in this browser', async ({ page, context }) => {
	await signIn(page);
	await page.goto('/settings#appearance');

	await page.getByRole('button', { name: /Sleepy/ }).click();
	// Give the debounced save a moment to land.
	await expect
		.poll(async () => {
			const res = await page.request.get('/api/settings');
			return (await res.json()).mascot;
		})
		.toBe('sleepy');

	// A browser with no local storage must still see the choice.
	const fresh = await context.newPage();
	await fresh.goto('/settings#appearance');
	const stored = await fresh.request.get('/api/settings');
	expect((await stored.json()).mascot).toBe('sleepy');
	await fresh.close();
});

test('score format changes how a rating reads without touching the value', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#watching');

	await page.getByRole('button', { name: /100 point/ }).click();
	await expect(page.getByRole('button', { name: 'Rated 78' })).toBeVisible();

	await page.getByRole('button', { name: /10 point decimal/ }).click();
	await expect(page.getByRole('button', { name: 'Rated 7.8' })).toBeVisible();

	// Ten with a decimal slides, from where the rating is, and goes no further
	// than the scale: End stops at 10, Home at 0, a step is a tenth.
	await page.getByRole('button', { name: 'Rated 7.8' }).click();
	const slider = page.getByRole('slider', { name: 'Score out of 10' });
	await expect(slider).toHaveAttribute('aria-valuetext', '7.8');
	await slider.press('End');
	await expect(slider).toHaveAttribute('aria-valuetext', '10.0');
	await slider.press('ArrowRight');
	await expect(slider).toHaveAttribute('aria-valuetext', '10.0');
	await slider.press('Home');
	await expect(slider).toHaveAttribute('aria-valuetext', '0.0');
	for (let i = 0; i < 75; i++) await slider.press('ArrowRight');
	await expect(slider).toHaveAttribute('aria-valuetext', '7.5');
	await slider.press('Enter');
	await expect(page.getByRole('button', { name: 'Rated 7.5' })).toBeVisible();
});

test('settings saved with a custom theme still load, and drop it on the next save', async ({
	page
}) => {
	await signIn(page);
	const saved = await (await page.request.get('/api/settings')).json();
	const custom = { name: 'Mine', base: 'pond', tokens: { accent: '#e06c9f', bg: '#101010' } };
	await page.request.put('/api/settings', {
		data: { ...saved, theme: 'pond', customTheme: custom, savedThemes: [custom] }
	});
	await page.goto('/settings#appearance');
	// The theme it was built on, none of its colours.
	await expect(page.getByRole('button', { name: 'Pond', exact: true })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	const inline = await page.evaluate(() =>
		document.documentElement.style.getPropertyValue('--accent')
	);
	expect(inline).toBe('');
	await expect(page.getByText('Colour editor')).toHaveCount(0);

	await page.getByRole('button', { name: 'Glass', exact: true }).click();
	await expect
		.poll(async () => Object.keys(await (await page.request.get('/api/settings')).json()))
		.not.toContain('customTheme');
	const after = await (await page.request.get('/api/settings')).json();
	expect(after.savedThemes).toBeUndefined();
	expect(after.theme).toBe('glass');
	await page.request.put('/api/settings', { data: saved });
});

test('search keeps filters in the address and tells kinds of result apart', async ({ page }) => {
	await signIn(page);
	await page.goto('/search?score=70&sort=rating');
	await expect(page.getByRole('tab', { name: 'Films' })).toBeVisible();
	await expect(page.getByRole('tab', { name: 'Characters' })).toBeVisible();
	await expect(page.getByRole('button', { name: /Filters/ })).toHaveAttribute(
		'aria-expanded',
		'true'
	);
	await page.getByRole('button', { name: 'Clear filters' }).click();
	await expect(page).not.toHaveURL(/score=/);
});

test('people has one search bar and no database talk', async ({ page }) => {
	await signIn(page);
	await page.goto('/people');
	await expect(page.getByRole('textbox', { name: 'Search people' })).toBeVisible();
});

test('settings no longer offers custom CSS', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#data');
	await expect(page.getByRole('button', { name: 'Reset all settings' })).toBeVisible();
	await expect(page.getByText('Custom CSS')).toHaveCount(0);
	await page.goto('/settings#appearance');
	await expect(page.getByText('Corner radius')).toHaveCount(0);
});

test('an address that leads nowhere offers a way home', async ({ page }) => {
	await signIn(page);
	await page.goto('/no-such-page');
	await expect(page.getByRole('heading', { name: 'Nothing here' })).toBeVisible();
	await page.getByRole('link', { name: 'Go home' }).click();
	await expect(page).toHaveURL('/');
});

test('a search survives a trip to another page and back', async ({ page }) => {
	await signIn(page);
	await page.goto('/search');
	await page.getByLabel('Search', { exact: true }).fill('heat');
	await expect(page).toHaveURL(/\/search\?q=heat$/);

	await page.goto('/stats');
	await page.goBack();
	await expect(page.getByLabel('Search', { exact: true })).toHaveValue('heat');
});

test('Back from a title opened by a click lands on the same search', async ({ page }) => {
	await signIn(page);
	await page.goto('/search');
	await page.getByLabel('Search', { exact: true }).fill('heat');
	await expect(page).toHaveURL(/\/search\?q=heat$/);

	// A link, not page.goto: a full reload never met this.
	await page.locator('a.brand').first().click();
	await expect(page).toHaveURL('/');
	await page.goBack();
	await expect(page).toHaveURL(/\/search\?q=heat$/);
	await expect(page.getByLabel('Search', { exact: true })).toHaveValue('heat');
});

/** A small library written straight into this run's database: there is no TMDB key here. */
function seedLibrary(project: string) {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const now = '2026-01-01T00:00:00Z';
	const { id: user } = db.prepare('SELECT id FROM app_user WHERE username = ?').get(USER) as {
		id: number;
	};
	const titles: [number, string, string, string, string, number][] = [
		[9101, 'movie', 'Alien', '["Horror"]', 'completed', 0],
		[9102, 'movie', 'Heat', '["Crime","Drama"]', 'completed', 1],
		[9103, 'tv', 'Dark', '["Drama"]', 'completed', 1],
		[9104, 'tv', 'Arcane', '["Action & Adventure"]', 'watching', 0],
		[9105, 'tv', 'Severance', '["Drama"]', 'planned', 0]
	];
	for (const [id, kind, name, genres, status, favourite] of titles) {
		db.prepare(
			`INSERT OR IGNORE INTO title (id, kind, name, genres_json, refreshed_at) VALUES (?, ?, ?, ?, ?)`
		).run(id, kind, name, genres, now);
		db.prepare(
			`INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
			 VALUES (?, 'tmdb', ?, 1, ?)`
		).run(id, `${kind}/${id}`, now);
		db.prepare(
			`INSERT OR REPLACE INTO library_entry (user_id, title_id, status, favourite, added_at, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?)`
		).run(user, id, status, favourite, now, now);
	}
	db.close();
}

function clearLibrary(project: string) {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	db.exec('DELETE FROM library_entry WHERE title_id BETWEEN 9101 AND 9105');
	db.close();
}

/** On a phone the filters are in a sheet; everywhere else they are on the page. */
async function filters(
	page: Page,
	isMobile: boolean | undefined,
	act: (at: Page) => Promise<void>
) {
	if (isMobile) await page.getByRole('button', { name: 'Filters', exact: false }).click();
	await act(page);
	if (isMobile) await page.getByRole('button', { name: /^Show \d+ title/ }).click();
}

const statusChip = (page: Page, name: RegExp) =>
	page.getByRole('group', { name: 'Status filter' }).getByRole('button', { name });
const titles = (page: Page) => page.locator('.grid a.poster');

test('the rating deck takes a slid score, skips, and takes both back', async ({
	page,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.goto('/rate');
	await expect(page.getByRole('heading', { name: 'Rate your library' })).toBeVisible();
	const togo = page.getByText(/\d+ to go/);
	await expect(togo).toBeVisible();
	const start = Number((await togo.innerText()).match(/(\d+) to go/)?.[1]);
	// The deck deals what was finished: Arcane, still being watched, and
	// Severance, on the watchlist, wait for Everything.
	expect(start).toBeGreaterThanOrEqual(3);
	await page.getByRole('button', { name: 'Everything', exact: true }).click();
	await expect(page.getByText(`${start + 2} to go`)).toBeVisible();
	await page.getByRole('button', { name: 'Finished', exact: true }).click();
	await expect(page.getByText(`${start} to go`)).toBeVisible();

	const card = page.locator('.card.top h2');
	const first = await card.innerText();
	const ratingOf = async (name: string) => {
		const res = await page.request.get('/api/library');
		const items: { name: string; entry: { rating: number | null } | null }[] = (await res.json())
			.items;
		return items.find((i) => i.name === name)?.entry?.rating ?? null;
	};

	// Ten with a decimal slides: it starts in the middle, a step is a tenth,
	// and the score given rates the card and brings up the next.
	const slider = page.getByRole('slider', { name: 'Score out of 10' });
	await expect(slider).toHaveAttribute('aria-valuetext', '5.0');
	for (let i = 0; i < 35; i++) await slider.press('ArrowRight');
	await expect(slider).toHaveAttribute('aria-valuetext', '8.5');
	await page.getByRole('button', { name: 'Rate', exact: true }).click();
	await expect(card).not.toHaveText(first);
	await expect(page.getByText(`${start - 1} to go`)).toBeVisible();
	await expect.poll(() => ratingOf(first)).not.toBeNull();

	await page.getByRole('button', { name: /^Skip/ }).click();
	await expect(page.getByText(`${start - 2} to go`)).toBeVisible();

	// Undo takes back the skip, then the rating.
	await page.getByRole('button', { name: /^Undo/ }).click();
	await expect(page.getByText(`${start - 1} to go`)).toBeVisible();
	await page.getByRole('button', { name: /^Undo/ }).click();
	await expect(card).toHaveText(first);
	await expect.poll(() => ratingOf(first)).toBeNull();

	if (isMobile) {
		// On a phone the finger throws the card: the browser scrolls it, and
		// letting go on a side skips it. One move a frame, as a finger reports,
		// once the card Undo brought back has landed.
		await page.waitForTimeout(400);
		const box = (await page.locator('.card.top').boundingBox())!;
		const cdp = await page.context().newCDPSession(page);
		const y = box.y + box.height / 2;
		const x = box.x + box.width / 2;
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
		for (let i = 1; i <= 12; i++) {
			await cdp.send('Input.dispatchTouchEvent', {
				type: 'touchMove',
				touchPoints: [{ x: x + i * 22, y }]
			});
			await page.waitForTimeout(16);
		}
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
		await expect(page.getByText(`${start - 1} to go`)).toBeVisible();
		await expect(card).not.toHaveText(first);
	}
	clearLibrary(testInfo.project.name);
});

test('library filters combine, and each status says what it would show', async ({
	page,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.goto('/library');
	await expect(titles(page)).toHaveCount(5);

	await filters(page, isMobile, async (at) => {
		await at.getByRole('button', { name: 'Type', exact: true }).click();
		await at.getByRole('option', { name: 'Shows', exact: true }).click();
		// With Shows on, Completed is worth 1 even though the library holds 3.
		await expect(statusChip(at, /Completed/)).toContainText('1');
		await statusChip(at, /Completed/).click();
	});
	await expect(page).toHaveURL(/status=completed/);
	await expect(page).toHaveURL(/kind=tv/);
	await expect(titles(page)).toHaveCount(1);
	await expect(titles(page).first()).toHaveAttribute('title', 'Dark');

	await filters(page, isMobile, async (at) => {
		await at.getByRole('button', { name: isMobile ? 'Clear all' : 'Clear', exact: true }).click();
	});
	await expect(page).toHaveURL('/library');
	await expect(titles(page)).toHaveCount(5);

	await filters(page, isMobile, async (at) => {
		await at.getByRole('button', { name: 'Genre', exact: true }).click();
		await at.getByRole('option', { name: 'Action & Adventure', exact: true }).click();
	});
	await expect(page).toHaveURL(/genre=Action/);
	await expect(titles(page)).toHaveCount(1);
	clearLibrary(testInfo.project.name);
});

test('a name sorts from A, and Back from a title keeps the filters', async ({
	page,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.goto('/library');
	await expect(titles(page)).toHaveCount(5);

	await filters(page, isMobile, async (at) => {
		await at.getByRole('button', { name: /^Sort/ }).click();
		await at.getByRole('option', { name: 'Name', exact: true }).click();
		await statusChip(at, /Completed/).click();
	});
	await expect(titles(page)).toHaveCount(3);
	await expect(titles(page).first()).toHaveAttribute('title', 'Alien');

	await titles(page).first().click();
	await expect(page).toHaveURL(/\/t\/9101/);
	await page.goBack();
	await expect(page).toHaveURL(/status=completed/);
	// Back draws the kept page and then the fresh answer over it; on a slow
	// phone runner the grid can be between the two for a moment.
	await expect(titles(page)).toHaveCount(3, { timeout: 15000 });
	await expect(titles(page).first()).toHaveAttribute('title', 'Alien', { timeout: 15000 });
	clearLibrary(testInfo.project.name);
});

/** A title page's answer as the server would give it, with `extra` on top. */
function titleAnswer(id: number, kind: 'movie' | 'tv', name: string, extra: object) {
	return {
		item: {
			id,
			kind,
			name,
			overview: null,
			poster_path: null,
			backdrop_path: null,
			logo_path: null,
			release_date: '2013-01-01',
			last_air_date: null,
			tmdb_status: null,
			runtime: null,
			episode_count: null,
			season_count: null,
			tmdb_vote: null,
			collection_id: null,
			genres: [],
			accent: { accent: '#6cc46b', shade: '#102018', tint: '#081008' },
			entry: null,
			watched_episodes: 0,
			total_episodes: 0,
			last_watched_at: null
		},
		tagline: null,
		homepage: null,
		cast: [],
		crew: [],
		trailers: [],
		providers: { flatrate: [], rent: [], buy: [], link: null },
		provider_region: 'BE',
		seasons: [],
		recommendations: [],
		similar: [],
		collection: null,
		next_up: null,
		ratings: [],
		...extra
	};
}

/** A film in the library, its saves answered by the page itself: `delay` holds each answer back. */
async function ratedFilm(
	page: Page,
	id: number,
	delay: (body: { rating?: number | null; status?: string }) => number = () => 0,
	extra: object = {}
) {
	const entry = {
		status: 'completed',
		rating: null as number | null,
		favourite: false,
		notes: null,
		started_at: null,
		finished_at: null,
		rewatch_count: 0,
		added_at: '2026-01-01T00:00:00Z',
		updated_at: '2026-01-01T00:00:00Z',
		notify: false
	};
	const answer: { item: Record<string, unknown> } = titleAnswer(id, 'movie', 'Rated Film', extra);
	answer.item = { ...answer.item, entry };
	const received: (number | null | undefined)[] = [];
	await page.route(`**/api/t/${id}`, (route) => route.fulfill({ json: answer }));
	await page.route(`**/api/t/${id}/scores`, (route) =>
		route.fulfill({ json: { ratings: [], age_rating: null } })
	);
	await page.route(`**/api/t/${id}/library`, async (route) => {
		const body = route.request().postDataJSON();
		received.push(body.rating);
		await new Promise((r) => setTimeout(r, delay(body)));
		const item = { ...answer.item, entry: { ...entry, ...body } };
		await route.fulfill({ json: { item } });
	});
	return received;
}

/** Rate with the smileys: three buttons, quick to tap. */
async function smileys(page: Page) {
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', { data: { ...saved, scoreFormat: 'smiley3' } });
	return () => page.request.put('/api/settings', { data: saved });
}

test('two quick ratings are saved in order, and the last one stays', async ({ page }) => {
	await signIn(page);
	const restore = await smileys(page);
	// The first answer is slow: it used to land last and put back the first score.
	const received = await ratedFilm(page, 9191, (body) => ((body.rating ?? 0) < 50 ? 900 : 0));
	await page.goto('/t/9191');
	const chip = page.locator('.hero .rating .chip');
	await chip.click();
	await page.getByRole('menuitemradio').nth(0).click();
	await chip.click();
	await page.getByRole('menuitemradio').nth(2).click();
	await expect(chip).toHaveAccessibleName('Rated 🙂');
	await page.waitForTimeout(1300);
	await expect(chip).toHaveAccessibleName('Rated 🙂');
	// And the server was asked in the order they were given.
	expect(received.map((r) => (r == null ? r : Math.round(r)))).toEqual([33, 100]);
	await restore();
});

test('a rating shows the moment it is chosen', async ({ page }) => {
	await signIn(page);
	const restore = await smileys(page);
	await ratedFilm(page, 9192, () => 2500);
	await page.goto('/t/9192');
	const chip = page.locator('.hero .rating .chip');
	await chip.click();
	await page.getByRole('menuitemradio').nth(1).click();
	await expect(chip).toHaveAccessibleName('Rated 😐', { timeout: 1000 });
	await restore();
});

test('tapping your own score keeps it; clearing is its own button', async ({ page }) => {
	await signIn(page);
	const restore = await smileys(page);
	const received = await ratedFilm(page, 9193);
	await page.goto('/t/9193');
	const chip = page.locator('.hero .rating .chip');
	await chip.click();
	await page.getByRole('menuitemradio').nth(1).click();
	await expect(chip).toHaveAccessibleName('Rated 😐');
	await chip.click();
	await page.getByRole('menuitemradio').nth(1).click();
	await expect(chip).toHaveAccessibleName('Rated 😐');
	await chip.click();
	await page.getByRole('button', { name: 'Clear' }).click();
	await expect(chip).toHaveAccessibleName('Rate this');
	expect(received.map((r) => (r == null ? r : Math.round(r)))).toEqual([67, 67, null]);
	await restore();
});

test('five stars are drawn, and read out as words', async ({ page }) => {
	await signIn(page);
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', { data: { ...saved, scoreFormat: 'star5' } });
	await ratedFilm(page, 9194);
	await page.goto('/t/9194');
	const chip = page.locator('.hero .rating .chip');
	await chip.click();
	const stars = page.getByRole('slider', { name: 'Stars' });
	await stars.press('End');
	await stars.press('ArrowLeft');
	await stars.press('Enter');
	await expect(chip).toHaveAccessibleName(/^Rated 4(½)? stars$/);
	await expect(chip.locator('svg').first()).toBeVisible();
	expect(await page.locator('body').innerText()).not.toContain('★');
	await page.request.put('/api/settings', { data: saved });
});

test('the rating menu opens whole, above everything, and listens only while open', async ({
	page,
	isMobile
}) => {
	// Count the window's listeners as the page adds and removes them.
	await page.addInitScript(() => {
		const live = new Map<string, number>();
		(window as unknown as { listening: Map<string, number> }).listening = live;
		const add = window.addEventListener.bind(window);
		const remove = window.removeEventListener.bind(window);
		window.addEventListener = ((type: string, ...rest: unknown[]) => {
			live.set(type, (live.get(type) ?? 0) + 1);
			return (add as (...a: unknown[]) => void)(type, ...rest);
		}) as typeof window.addEventListener;
		window.removeEventListener = ((type: string, ...rest: unknown[]) => {
			live.set(type, (live.get(type) ?? 0) - 1);
			return (remove as (...a: unknown[]) => void)(type, ...rest);
		}) as typeof window.removeEventListener;
	});
	await signIn(page);
	const restore = await smileys(page);
	await ratedFilm(page, 9195);
	const keydowns = () =>
		page.evaluate(
			() => (window as unknown as { listening: Map<string, number> }).listening.get('keydown') ?? 0
		);
	await page.goto('/t/9195');
	const chip = page.locator('.hero .rating .chip');
	await expect(chip).toBeVisible();
	const closed = await keydowns();
	await chip.click();
	const menu = page.getByRole('menu', { name: 'Choose a rating' });
	await expect(menu).toBeVisible();
	expect(await keydowns()).toBeGreaterThan(closed);
	// Measured once it has finished rising into place.
	await page.waitForTimeout(400);

	// Whole on the screen, and nothing drawn over it, the tab bar included.
	const box = (await menu.boundingBox())!;
	const view = page.viewportSize()!;
	expect(box.y).toBeGreaterThanOrEqual(0);
	expect(box.y + box.height).toBeLessThanOrEqual(view.height);
	expect(box.x).toBeGreaterThanOrEqual(0);
	expect(box.x + box.width).toBeLessThanOrEqual(view.width);
	const onTop = await page.evaluate(
		([x, y]) => !!document.elementFromPoint(x, y)?.closest('[role="menu"]'),
		[box.x + box.width / 2, box.y + box.height - 4]
	);
	expect(onTop, isMobile ? 'the sheet is over the tab bar' : 'the menu is on top').toBe(true);

	await page.keyboard.press('Escape');
	await expect(menu).toBeHidden();
	expect(await keydowns()).toBe(closed);
	await restore();
});

test('the shelves menu stays on the screen and never widens the page', async ({
	page,
	isMobile
}) => {
	await signIn(page);
	await ratedFilm(page, 9197);
	await page.route('**/api/t/9197/shelves', (route) =>
		route.fulfill({
			json: {
				shelves: [
					{ id: 1, name: 'Sunday nights with a very long shelf name indeed', on: true },
					{ id: 2, name: 'Comfort', on: false }
				]
			}
		})
	);
	await page.goto('/t/9197');
	await page.getByRole('button', { name: 'Add to shelf' }).click();
	const menu = page.getByRole('menu', { name: 'Shelves' });
	await expect(menu).toBeVisible();
	await page.waitForTimeout(400);
	const box = (await menu.boundingBox())!;
	const view = page.viewportSize()!;
	expect(box.x).toBeGreaterThanOrEqual(0);
	expect(box.x + box.width).toBeLessThanOrEqual(view.width);
	expect(box.y + box.height).toBeLessThanOrEqual(view.height);
	// Anchored, it ran past the right edge and the page panned sideways.
	const wide = await page.evaluate(() => ({
		scroll: document.documentElement.scrollWidth,
		client: document.documentElement.clientWidth
	}));
	expect(wide.scroll).toBe(wide.client);
	if (isMobile) {
		// A sheet along the bottom, like a card's menu.
		expect(Math.round(box.y + box.height)).toBe(view.height);
	}
	await expect(page.getByRole('menuitemcheckbox', { name: 'Comfort' })).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menu).toBeHidden();
});

test("a title's picture keeps its size when a status or rating is saved", async ({ page }) => {
	await signIn(page);
	// Short enough that the words over the picture are taller than its least height.
	await page.setViewportSize({ width: page.viewportSize()!.width, height: 420 });
	await page.route('**/img/w1280/frog-test-backdrop.jpg', (route) =>
		route.fulfill({
			contentType: 'image/svg+xml',
			body: '<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720"><rect width="1280" height="720" fill="#386"/></svg>'
		})
	);
	const answer: { item: Record<string, unknown> } = titleAnswer(9196, 'movie', 'Pictured Film', {
		tagline: 'A line long enough to take up some room over the picture, as taglines do.'
	});
	answer.item = { ...answer.item, backdrop_path: '/frog-test-backdrop.jpg' };
	await page.route('**/api/t/9196', (route) => route.fulfill({ json: answer }));
	await page.route('**/api/t/9196/scores', (route) =>
		route.fulfill({ json: { ratings: [], age_rating: null } })
	);
	await page.route('**/api/t/9196/library', (route) => {
		const entry = {
			status: 'watching',
			rating: null,
			favourite: false,
			notes: null,
			started_at: null,
			finished_at: null,
			rewatch_count: 0,
			added_at: '2026-01-01T00:00:00Z',
			updated_at: '2026-01-01T00:00:00Z',
			notify: false,
			...route.request().postDataJSON()
		};
		return route.fulfill({ json: { item: { ...answer.item, entry } } });
	});
	await page.goto('/t/9196');
	const picture = page.locator('.hero img.backdrop');
	await expect(picture).toBeVisible();
	// The laid-out height: the picture's entrance scales it for a moment.
	const height = () => picture.evaluate((el) => (el as HTMLElement).offsetHeight);
	const before = await height();
	const hero = await page.locator('.hero').evaluate((el) => (el as HTMLElement).offsetHeight);
	expect(hero, 'the words are taller than the picture here').toBeGreaterThan(before);
	// Tracking it changes what sits over the picture; the picture stays.
	await page.getByRole('button', { name: 'Start watching' }).click();
	await expect(page.getByRole('button', { name: 'Status' })).toBeVisible();
	expect(await height()).toBe(before);
});

test('a title whose provider repeats itself still opens', async ({ page }) => {
	// This suite runs the production build, where a repeated key in a list
	// throws and takes the page with it. Monsters vs. Aliens did exactly that.
	await signIn(page);
	const person = { id: 5727, name: 'Diedrich Bader', character: 'Coverton', profile_path: null };
	const film = (name: string) => ({
		kind: 'movie',
		tmdb_id: 7,
		id: null,
		name,
		poster_path: null,
		backdrop_path: null,
		year: 2001,
		overview: null,
		vote: null,
		in_library: null
	});
	const trailer = { key: 't', name: 'Trailer', site: 'YouTube', kind: 'Trailer', official: true };
	const provider = { id: 8, name: 'Netflix', logo_path: null };
	const season = { season_number: 1, name: 'Season 1', episode_count: 1 };
	const answer = titleAnswer(9199, 'tv', 'Repeats', {
		cast: [person, { ...person, character: 'The Missing Link' }],
		crew: [{ id: 1, name: 'Maker', job: 'Creator', profile_path: null }],
		trailers: [trailer, trailer],
		providers: { flatrate: [provider, provider], rent: [], buy: [], link: null },
		seasons: [season, season],
		recommendations: [film('One'), film('One')],
		collection: { id: 1, name: 'Set', parts: [film('Part'), film('Part')] }
	});
	await page.route('**/api/t/9199', (route) => route.fulfill({ json: answer }));
	await page.route('**/api/t/9199/scores', (route) =>
		route.fulfill({ json: { ratings: [], age_rating: null } })
	);
	await page.route('**/api/t/9199/seasons/1', (route) =>
		route.fulfill({
			json: {
				season_number: 1,
				episodes: [1, 1].map(() => ({
					season_number: 1,
					episode_number: 1,
					name: 'Pilot',
					watched: false,
					watch_count: 0,
					undated: false
				}))
			}
		})
	);

	const crashes: string[] = [];
	page.on('pageerror', (e) => crashes.push(e.message));
	await page.goto('/t/9199');
	await expect(page.getByRole('heading', { name: 'Repeats' })).toBeVisible();
	await expect(page.locator('a.person')).toHaveCount(1);
	await expect(page.locator('.plogo')).toHaveCount(1);
	await expect(page.locator('.trailer')).toHaveCount(1);
	await expect(page.locator('.episode')).toHaveCount(1);
	expect(crashes).toEqual([]);
});

test('edit mode ticks titles in every view instead of opening them', async ({
	page,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	const saved = await (await page.request.get('/api/settings')).json();
	const views = [
		{ card: 'poster', first: '.grid a.poster' },
		{ card: 'landscape', first: 'a.land' },
		{ card: 'list', first: '.list a' },
		{ card: 'compact', first: 'tbody a' }
	];
	for (const { card, first } of views) {
		await page.request.put('/api/settings', { data: { ...saved, cardStyle: card } });
		await page.goto('/library');
		await page.getByRole('button', { name: 'Edit', exact: true }).click();
		await page.locator(first).first().click();
		await expect(page, `${card} stays on the list`).toHaveURL('/library');
		await expect(page.getByText('1 selected')).toBeVisible();
		await page.getByRole('button', { name: 'Select all' }).click();
		await expect(page.getByText('5 selected')).toBeVisible();
		await page.getByRole('button', { name: 'Select none' }).click();
		await expect(page.getByText('0 selected')).toBeVisible();
		await page.getByRole('button', { name: 'Done', exact: true }).click();
		await page.locator(first).first().click();
		await expect(page, `${card} opens again once done`).toHaveURL(/\/t\/\d+/);
	}
	await page.request.put('/api/settings', { data: saved });

	// Select all covers what the filters show, not the whole library.
	await page.goto('/library?kind=movie');
	await page.getByRole('button', { name: 'Edit', exact: true }).click();
	await page.getByRole('button', { name: 'Select all' }).click();
	await expect(page.getByText('2 selected')).toBeVisible();

	if (isMobile) {
		const bar = page.getByRole('toolbar', { name: 'Edit your library' });
		const fits = await bar.evaluate((el) => el.getBoundingClientRect().right <= innerWidth);
		expect(fits, 'the bar fits a phone').toBe(true);
	}
	clearLibrary(testInfo.project.name);
});

test('removing many titles is one request, and undo brings back every field', async ({
	page
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.request.put('/api/t/9102/library', {
		data: { rating: 91, notes: 'Saw it twice', status: 'paused', favourite: true }
	});
	const before = await (await page.request.get('/api/library?sort=name&order=asc')).json();
	const held = (answer: { items: { id: number; entry: unknown }[] }) =>
		answer.items.find((i) => i.id === 9102)?.entry;

	await page.goto('/library');
	await page.getByRole('button', { name: 'Edit', exact: true }).click();
	await page.getByRole('button', { name: 'Select all' }).click();

	const sent: string[] = [];
	page.on('request', (r) => {
		if (r.method() !== 'GET' && r.url().includes('/api/')) sent.push(r.url());
	});
	await page.getByRole('button', { name: 'Remove' }).click();
	await expect(page.getByText('Removed 5 titles')).toBeVisible();
	await expect(titles(page)).toHaveCount(0);
	expect(sent.map((u) => new URL(u).pathname)).toEqual(['/api/library/bulk']);

	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(titles(page)).toHaveCount(5);
	const after = await (await page.request.get('/api/library?sort=name&order=asc')).json();
	expect(held(after)).toEqual(held(before));
	clearLibrary(testInfo.project.name);
});

test('marking many titles watched asks once', async ({ page }, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.goto('/library?kind=movie');
	await page.getByRole('button', { name: 'Edit', exact: true }).click();
	await page.getByRole('button', { name: 'Select all' }).click();
	await page.getByRole('button', { name: 'Watched', exact: true }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog.getByText('When did you watch this?')).toBeVisible();
	await dialog.getByRole('radio', { name: /I don't know/ }).click();
	await dialog.getByRole('button', { name: 'Mark watched' }).click();
	await expect(dialog).toHaveCount(0);
	await expect(page.getByText('Marked 2 titles watched')).toBeVisible();
	const diary = await (await page.request.get('/api/diary')).json();
	expect(diary.undated).toBeGreaterThanOrEqual(2);

	const db = new DatabaseSync(join('.e2e-data', testInfo.project.name, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	db.exec('DELETE FROM watch_event WHERE title_id BETWEEN 9101 AND 9105');
	db.close();
	clearLibrary(testInfo.project.name);
});

test('labels on a title card never overlap, on the narrowest phone', async ({ page }) => {
	await signIn(page);
	await page.setViewportSize({ width: 320, height: 640 });
	const item = (n: number, kind: string, in_library: string | null) => ({
		kind,
		tmdb_id: n,
		id: null,
		name: `Title ${n}`,
		poster_path: null,
		backdrop_path: null,
		year: 2020,
		overview: null,
		vote: null,
		popularity: 1,
		vote_count: 1,
		genre_ids: [],
		in_library
	});
	const statuses = ['watching', 'completed', 'planned', 'paused', 'dropped', null];
	const titles = statuses.flatMap((status, i) => [
		item(i * 2 + 1, 'movie', status),
		item(i * 2 + 2, 'tv', status)
	]);
	await page.route('**/api/studios/7', (route) =>
		route.fulfill({ json: { id: 7, name: 'A Studio', titles } })
	);
	await page.route('**/api/search?*', (route) =>
		route.fulfill({ json: { results: titles, next_page: null } })
	);
	await page.route('**/api/search/more?*', (route) =>
		route.fulfill({ json: { people: [], characters: [], studios: [] } })
	);

	const overlapping = () =>
		page.evaluate(() => {
			const clash = (a: DOMRect, b: DOMRect) =>
				a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
			const found: string[] = [];
			for (const card of document.querySelectorAll('.art')) {
				const host = card.closest('.result') ?? card;
				const labels = [...host.querySelectorAll('.badge, .quick .q')].filter(
					(el) => (el as HTMLElement).offsetWidth > 0
				);
				if (labels.length < 2) continue;
				labels.forEach((a, i) =>
					labels.slice(i + 1).forEach((b) => {
						if (clash(a.getBoundingClientRect(), b.getBoundingClientRect())) {
							found.push(`${a.textContent?.trim()} and ${b.textContent?.trim() || 'the button'}`);
						}
					})
				);
			}
			return { cards: document.querySelectorAll('.art').length, found };
		});

	await page.goto('/studio/7');
	await expect(page.getByText('Title 12').first()).toBeVisible();
	const onStudio = await overlapping();
	expect(onStudio.cards).toBe(12);
	expect(onStudio.found).toEqual([]);

	await page.goto('/search?q=title&type=movies');
	await expect(page.getByText('Title 11').first()).toBeVisible();
	const onSearch = await overlapping();
	expect(onSearch.cards).toBeGreaterThan(0);
	expect(onSearch.found).toEqual([]);
});

test("someone's whole library opens from their profile, without their notes", async ({
	page,
	playwright,
	baseURL
}, testInfo) => {
	await signIn(page);
	const name = `friend${Date.now() % 100000}`;
	const invite = await (await page.request.post('/api/invites', { data: { role: 'user' } })).json();
	const friend = await playwright.request.newContext({ baseURL });
	await friend.post('/api/auth/register', {
		data: { username: name, password: 'another-long-pass', invite: invite.code, remember: true }
	});
	await friend.put('/api/account/profile', { data: { public: true } });

	const secret = 'Only for me to read';
	const db = new DatabaseSync(join('.e2e-data', testInfo.project.name, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const now = '2026-01-01T00:00:00Z';
	const { id: user } = db.prepare('SELECT id FROM app_user WHERE username = ?').get(name) as {
		id: number;
	};
	for (let id = 9300; id < 9370; id++) {
		db.prepare(
			`INSERT OR IGNORE INTO title (id, kind, name, genres_json, refreshed_at) VALUES (?, 'movie', ?, '[]', ?)`
		).run(id, `Shared ${id}`, now);
		db.prepare(
			`INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
			 VALUES (?, 'tmdb', ?, 1, ?)`
		).run(id, `movie/${id}`, now);
		db.prepare(
			`INSERT OR REPLACE INTO library_entry (user_id, title_id, status, notes, added_at, updated_at)
			 VALUES (?, ?, 'completed', ?, ?, ?)`
		).run(user, id, secret, now, now);
	}
	db.close();

	const answers: string[] = [];
	page.on('response', async (r) => {
		if (r.url().includes('/library')) answers.push(await r.text().catch(() => ''));
	});

	await page.goto(`/people/${name}`);
	await page
		.getByRole('navigation', { name: 'Profile' })
		.getByRole('link', { name: 'Library' })
		.click();
	await expect(page).toHaveURL(new RegExp(`/people/${name}/library`));
	await expect(titles(page)).toHaveCount(60);
	await expect(page.getByText('70 titles')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Edit', exact: true })).toHaveCount(0);

	// Further down, the next page. Reading on is not a button to find.
	await page.locator('.more').scrollIntoViewIfNeeded();
	await expect(titles(page)).toHaveCount(70);
	expect(answers.join('')).not.toContain(secret);

	// The viewer's own page for the title, not theirs.
	await titles(page).first().click();
	await expect(page).toHaveURL(/\/t\/93\d\d/);

	// Turned off, it reads as if it was never there.
	await friend.put('/api/account/profile', { data: { public: false } });
	await page.goto(`/people/${name}/library`);
	await expect(page.getByRole('heading', { name: 'No profile here' })).toBeVisible();
	const [off, none] = await Promise.all([
		page.request.get(`/api/profiles/${name}/library`),
		page.request.get('/api/profiles/nobody-at-all/library')
	]);
	expect(off.status()).toBe(404);
	expect(await off.text()).toBe(await none.text());
	await friend.dispose();
});

test('a page that cannot load says so, and can try again', async ({ page }) => {
	await signIn(page);
	for (const [path, api] of [
		['/', '**/api/home'],
		['/diary', '**/api/diary?*'],
		['/calendar', '**/api/calendar'],
		['/airing', '**/api/airing?*'],
		['/stats', '**/api/stats'],
		['/shelves', '**/api/shelves'],
		['/people', '**/api/profiles']
	]) {
		await page.route(api, (route) =>
			route.fulfill({ status: 500, json: { error: 'database is locked' } })
		);
		await page.goto(path);
		const again = page.getByRole('button', { name: 'Try again' });
		await expect(again, `${path} offers another go`).toBeVisible();
		await expect(page.locator('main')).not.toContainText('database');
		await page.unroute(api);
		await again.click();
		await expect(again, `${path} recovers`).toBeHidden();
	}
});

test('renaming a shelf starts in the name field', async ({ page }) => {
	await signIn(page);
	const created = await page.request.post('/api/shelves', {
		data: { name: 'Rename me', kind: 'manual' }
	});
	const { shelf } = await created.json();
	await page.goto(`/shelves/${shelf.id}`);

	await page.getByRole('button', { name: 'Rename' }).click();
	await page.keyboard.type('Sunday films');
	await page.keyboard.press('Enter');
	await expect(page.getByRole('heading', { name: 'Sunday films' })).toBeVisible();

	// Escape leaves the name as it was.
	await page.getByRole('button', { name: 'Rename' }).click();
	await page.keyboard.type('Never mind');
	await page.keyboard.press('Escape');
	await expect(page.getByRole('heading', { name: 'Sunday films' })).toBeVisible();

	await page.request.delete(`/api/shelves/${shelf.id}`);
});

test('a right-click on a title offers its actions, each with an undo', async ({
	page,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	await page.goto('/library?sort=name&order=asc');
	const first = titles(page).first();
	await expect(first).toHaveAttribute('title', 'Alien');

	// A right-click on a computer; a hold on a phone, which the browser
	// reports as the same event.
	const open = async () => {
		if (isMobile) await first.dispatchEvent('contextmenu');
		else await first.click({ button: 'right' });
		await expect(page.getByRole('menu', { name: 'Alien' })).toBeVisible();
	};

	await open();
	await expect(page, 'the card did not open').toHaveURL(/\/library/);
	const menu = page.getByRole('menu', { name: 'Alien' });
	await expect(menu.getByRole('menuitem', { name: 'Remove from library' })).toBeVisible();

	// On a computer a choice's own choices open beside it on hover.
	if (!isMobile) {
		await menu.getByRole('menuitem', { name: 'Move to' }).hover();
		await expect(menu.getByRole('menu', { name: 'Move to' })).toBeVisible();
	}

	// A second level in the same menu, and the current status is ticked.
	await menu.getByRole('menuitem', { name: 'Move to' }).click();
	await expect(menu.getByRole('menuitemcheckbox', { name: 'Completed' })).toHaveAttribute(
		'aria-checked',
		'true'
	);
	await menu.getByRole('menuitemcheckbox', { name: 'Watching' }).click();
	await expect(page.getByText('Moved Alien to Watching')).toBeVisible();
	await expect(menu).toHaveCount(0);
	await expect(first.getByText('Watching')).toBeVisible();
	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(first.getByText('Completed')).toBeVisible();

	// Escape closes it without doing anything.
	await open();
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);

	await open();
	await menu.getByRole('menuitem', { name: 'Remove from library' }).click();
	await expect(page.getByText('Removed Alien')).toBeVisible();
	await expect(titles(page)).toHaveCount(4);
	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(titles(page)).toHaveCount(5);
	clearLibrary(testInfo.project.name);
});

test('the next account on this device starts from its own preferences', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#appearance');
	await page.getByRole('button', { name: /^Light$/ }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

	const made = await page.request.post('/api/invites', { data: { role: 'user' } });
	const invite = await made.json();

	await page.getByRole('button', { name: /^Account:/ }).click();
	await page.getByRole('menuitem', { name: 'Switch account' }).click();
	await expect(page).toHaveURL(/\/login/);

	// Joining is the other way a second account arrives on a device.
	const name = `guest${Date.now() % 100000}`;
	await page.goto(invite.path);
	await page.getByLabel('Username').fill(name);
	await page.getByLabel('Password').fill('another-long-pass');
	await page.getByRole('button', { name: 'Create my account' }).click();
	await expect(page.getByRole('heading', { name: `Welcome, ${name}` })).toBeVisible();

	await expect(page.locator('html')).toHaveAttribute('data-theme', 'glass');
	const theirs = await (await page.request.get('/api/settings')).json();
	expect(theirs.theme ?? 'glass').toBe('glass');

	// Back to the first account, as it was.
	await page.request.post('/api/auth/logout', { data: {} });
	await signIn(page);
	const mine = await (await page.request.get('/api/settings')).json();
	expect(mine.theme).toBe('light');
	await page.request.put('/api/settings', { data: { ...mine, theme: 'glass' } });
});

test('controls are big enough to touch on a phone', async ({ page, isMobile }) => {
	test.skip(!isMobile, 'touch targets only apply to the phone layout');
	await signIn(page);
	for (const path of ['/library', '/settings#home', '/shelves', '/stats', '/calendar', '/search']) {
		await page.goto(path);
		await page.waitForLoadState('networkidle');
		const small = await page.evaluate(() =>
			[
				...document.querySelectorAll(
					'main .btn, main .icon-btn, main .chip, main .trigger, main .switch button'
				)
			]
				.map((el) => ({ el, r: el.getBoundingClientRect() }))
				.filter(({ r }) => r.width > 0 && (r.height < 44 || r.width < 44))
				.map(({ el, r }) => `${el.textContent?.trim() || el.className} ${r.width}x${r.height}`)
		);
		expect(small, `${path} has controls under 44px`).toEqual([]);
	}
});

test('a shelf shows once on Home, in the row at the top', async ({ page }, testInfo) => {
	await signIn(page);
	seedLibrary(testInfo.project.name);
	const made = await page.request.post('/api/shelves', {
		data: { name: 'Keepers', kind: 'manual', title_ids: [9102] }
	});
	const { shelf } = await made.json();

	// A layout saved with a row for this one shelf, as older ones have.
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', {
		data: { ...saved, homeRows: ['shelves', 'continue', `shelf:${shelf.id}`] }
	});

	// Twice: the first visit stores the layout in this browser, as a person's has.
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Your shelves' })).toBeVisible();
	// Rows for one shelf arrive after the rest, so this has to wait for them.
	await page.waitForLoadState('networkidle');
	await expect(page.getByText('Keepers')).toHaveCount(1);

	await page.goto(`/shelves/${shelf.id}`);
	await expect(page.getByRole('heading', { name: 'Keepers' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Show on Home' })).toHaveCount(0);

	await page.request.delete(`/api/shelves/${shelf.id}`);
	clearLibrary(testInfo.project.name);
});

test('signing out closes the door behind you', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#data');
	// Each signed-in device has its own Sign out too, and the header menu
	// carries one; this is the one that ends the session you are using.
	await page.getByRole('main').getByRole('button', { name: 'Sign out', exact: true }).click();

	await expect(page).toHaveURL(/\/login/);

	// Going straight back to a private route must not get in.
	await page.goto('/stats');
	await expect(page).toHaveURL(/\/login/);
});

test('the app is installable: manifest and icons are served', async ({ request }) => {
	const manifest = await request.get('/manifest.webmanifest');
	expect(manifest.ok()).toBeTruthy();

	const json = await manifest.json();
	expect(json.name).toContain('Frog');
	expect(json.icons.length).toBeGreaterThanOrEqual(2);

	for (const icon of json.icons) {
		const res = await request.get(icon.src);
		expect(res.ok(), `${icon.src} should be served`).toBeTruthy();
	}
});

test('images and the API stay behind the auth wall', async ({ request }) => {
	for (const path of [
		'/api/library',
		'/api/library/stats',
		'/api/home',
		'/api/settings',
		'/api/calendar',
		'/api/airing',
		'/api/account/merges',
		'/api/t/1/more',
		'/api/airing/day?date=2026-11-14',
		'/api/diary',
		'/api/export/library.json',
		'/api/auth/two-factor',
		'/api/stats',
		'/api/stats/manga',
		'/api/stats?year=2025',
		'/api/push',
		'/api/shelves',
		'/api/shelves/1',
		'/api/titles/movie/1',
		'/api/titles/anilist/1',
		'/api/t/1',
		'/api/t/1/seasons/1',
		'/api/t/1/shelves',
		'/api/home/because',
		'/api/home/featured',
		'/api/home/activity',
		'/api/activity',
		'/api/account',
		'/api/account/sessions',
		'/api/account/tokens',
		'/api/extension/link',
		'/api/ext/me',
		'/api/admin/users',
		'/api/admin/instance',
		'/api/admin/server',
		'/api/admin/db/tables',
		'/api/admin/db/tables/app_user',
		'/api/admin/badges',
		'/api/admin/badges/1',
		'/api/admin/badges/1/original',
		'/api/admin/badges/1/images/large',
		'/api/account/badges',
		'/api/profiles/someone/badges/1/small',
		'/api/profiles/someone/badges',
		'/api/badges/1/small',
		'/api/profiles',
		'/api/profiles/someone',
		'/api/profiles/someone/library',
		'/api/profiles/someone/ratings',
		'/api/profiles/someone/compare',
		'/api/profiles/someone/images/avatar',
		'/api/discover/trending?list=top',
		'/api/anilist',
		'/api/anilist/entries',
		'/api/invites',
		'/api/search/more?q=frog',
		'/api/search/genres',
		'/api/search/backdrops/movie/550',
		'/api/t/1/scores',
		'/api/studios/1',
		'/api/notifications',
		'/api/notifications/unread',
		'/api/notifications/settings',
		'/api/people/choosable',
		'/img/w342/nope.jpg'
	]) {
		const res = await request.get(path);
		expect(res.status(), `${path} should be 401`).toBe(401);
	}
	for (const path of [
		'/api/library/bulk',
		'/api/auth/two-factor/start',
		'/api/auth/two-factor/confirm',
		'/api/auth/two-factor/disable',
		'/api/auth/two-factor/recovery-codes',
		'/api/diary/redate',
		'/api/import/match',
		'/api/import/apply',
		'/api/push/subscribe',
		'/api/push/unsubscribe',
		'/api/shelves',
		'/api/shelves/1/items',
		'/api/invites',
		'/api/anilist/connect',
		'/api/anilist/sync',
		'/api/t/1/episodes/watch',
		'/api/t/1/episodes/1/watch',
		'/api/t/1/episodes/1/1/watch',
		'/api/t/1/rewatch',
		'/api/admin/badges',
		'/api/admin/badges/restore',
		'/api/account/tokens',
		'/api/ext/resolve',
		'/api/ext/log',
		'/api/ext/undo',
		'/api/notifications/read',
		'/api/notifications/restore',
		'/api/t/1/recommend',
		'/api/recommendations/1/answer'
	]) {
		const res = await request.post(path, { data: {} });
		expect(res.status(), `POST ${path} should be 401`).toBe(401);
	}
	for (const path of [
		'/api/t/1/seasons/1/rating',
		'/api/t/1/library',
		'/api/t/1/episodes/1/1/state',
		'/api/anilist/entries/1',
		'/api/admin/badges/1',
		'/api/admin/badges/1/holders/1',
		'/api/admin/badges/1/condition',
		'/api/account/badges',
		'/api/account/device'
	]) {
		const res = await request.put(path, { data: { rating: 80 } });
		expect(res.status(), `PUT ${path} should be 401`).toBe(401);
	}
	const reset = await request.post('/api/admin/users/1/reset', { data: {} });
	expect(reset.status(), 'POST a reset link should be 401').toBe(401);
	const rename = await request.put('/api/account/username', {
		data: { username: 'nobody', password: 'a-long-enough-pass' }
	});
	expect(rename.status(), 'PUT a username should be 401').toBe(401);

	for (const path of [
		'/api/account',
		'/api/account/library',
		'/api/t/1/library',
		'/api/admin/badges/1',
		'/api/admin/badges/1/holders/1',
		'/api/account/tokens/1',
		'/api/notifications',
		'/api/notifications/1'
	]) {
		const res = await request.delete(path, { data: {} });
		expect(res.status(), `DELETE ${path} should be 401`).toBe(401);
	}

	// The Firefox build answers only a link Settings gave out.
	for (const path of ['/extension/frog.xpi', '/extension/frog.xpi?key=guessed']) {
		expect((await request.get(path)).status(), `${path} should be 404`).toBe(404);
	}

	// Joining is open by design, so it cannot be behind the wall. Without a
	// live invite it still refuses, which is what keeps sign-up closed.
	const join = await request.post('/api/auth/register', {
		data: { username: 'nobody', password: 'a-long-enough-pass', invite: 'made-up' }
	});
	expect(join.status(), 'registering without a real invite is refused').toBe(403);

	// The same for a reset link: the person using one cannot sign in, so the
	// route stays open and a code is what it asks for.
	const madeUp = await request.post('/api/auth/reset', {
		data: { code: 'made-up', password: 'a-long-enough-pass' }
	});
	expect(madeUp.status(), 'resetting without a real link is refused').toBe(403);
});

test('a reset link sets a new password, once', async ({ page, browser, playwright, baseURL }) => {
	await signIn(page);
	const name = `forgot${Date.now() % 100000}`;
	const invite = await (await page.request.post('/api/invites', { data: { role: 'user' } })).json();
	const them = await playwright.request.newContext({ baseURL });
	await them.post('/api/auth/register', {
		data: { username: name, password: 'another-long-pass', invite: invite.code, remember: true }
	});

	await page.goto('/admin');
	await page.getByRole('button', { name: `Reset link for ${name}` }).click();
	const link = await page.locator('.fresh.reset code').textContent();
	expect(link).toContain('/reset?code=');

	const stranger = await browser.newContext({ baseURL });
	const seen = await enforceDeployedCsp(stranger, baseURL ?? '');
	const tab = await stranger.newPage();
	await tab.goto(link ?? '');
	await expect(tab).toHaveTitle('Reset password · Frog');
	await tab.getByLabel('New password').fill('a-brand-new-pass');
	await tab.getByRole('button', { name: 'Save password' }).click();
	await expect(tab.getByRole('heading', { name: 'Password changed' })).toBeVisible();

	// Their device is signed out, and the same link does not work twice.
	expect((await them.get('/api/account')).status()).toBe(401);
	await tab.goto(link ?? '');
	await tab.getByLabel('New password').fill('yet-another-pass');
	await tab.getByRole('button', { name: 'Save password' }).click();
	await expect(tab.getByRole('alert')).toContainText('used already');

	await tab.goto('/reset');
	await expect(tab.getByText('This page needs a reset link.')).toBeVisible();

	await tab.goto('/login');
	await tab.getByLabel('Username').fill(name);
	await tab.getByLabel('Password').fill('a-brand-new-pass');
	await tab.getByRole('button', { name: 'Sign in' }).click();
	await expect(tab).toHaveURL(/\/(welcome)?$/);
	expect(seen, 'blocked by the deployed Content-Security-Policy').toEqual([]);
	await stranger.close();
	await them.dispose();
});

test('renaming yourself shows at once, everywhere', async ({
	browser,
	playwright,
	baseURL,
	page
}) => {
	await signIn(page);
	const stamp = Date.now() % 100000;
	const invite = await (await page.request.post('/api/invites', { data: { role: 'user' } })).json();
	const them = await playwright.request.newContext({ baseURL });
	await them.post('/api/auth/register', {
		data: {
			username: `before${stamp}`,
			password: 'another-long-pass',
			invite: invite.code,
			remember: true
		}
	});
	await them.put('/api/settings', { data: { onboarded: true } });
	const context = await browser.newContext({ baseURL, storageState: await them.storageState() });
	const tab = await context.newPage();

	await tab.goto('/settings#account');
	const row = tab.locator('.row', { hasText: 'It is how you sign in' });
	await row.getByRole('button', { name: 'Change' }).click();
	await expect(row.getByLabel('New username')).toHaveAttribute('maxlength', '20');
	await row.getByLabel('New username').fill(`after${stamp}`);
	await row.getByLabel('Current password').fill('another-long-pass');
	await row.getByRole('button', { name: 'Save username' }).click();

	await expect(tab.getByRole('button', { name: `Account: after${stamp}` })).toBeVisible();
	await expect(tab.getByText(`after${stamp}`, { exact: true }).first()).toBeVisible();
	await tab.getByRole('button', { name: `Account: after${stamp}` }).click();
	await tab.getByRole('menuitem', { name: 'Your profile' }).click();
	await expect(tab).toHaveURL(`/people/after${stamp}`);
	await context.close();
	await them.dispose();
});

/** Picture bytes made in the page, since nothing here ships a large photo. */
async function pictureFile(page: Page, width: number, height: number, type: string) {
	const base64 = await page.evaluate(
		async ({ width, height, type }) => {
			const canvas = document.createElement('canvas');
			canvas.width = width;
			canvas.height = height;
			const context = canvas.getContext('2d')!;
			// Half left clear, so transparency has something to survive on.
			for (let n = 0; n < 4000; n++) {
				context.fillStyle = `hsl(${n % 360} 80% 50%)`;
				context.fillRect((n * 7919) % (width / 2), (n * 104729) % height, 40, 40);
			}
			const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, type));
			const bytes = new Uint8Array(await blob!.arrayBuffer());
			let out = '';
			for (let i = 0; i < bytes.length; i += 8192) {
				out += String.fromCharCode(...bytes.subarray(i, i + 8192));
			}
			return btoa(out);
		},
		{ width, height, type }
	);
	return Buffer.from(base64, 'base64');
}

/** The picture the server now holds, as the browser reads it. */
function readPicture(page: Page, address: string) {
	return page.evaluate(async (address) => {
		const blob = await (await fetch(address)).blob();
		const bitmap = await createImageBitmap(blob);
		const canvas = document.createElement('canvas');
		canvas.width = bitmap.width;
		canvas.height = bitmap.height;
		const context = canvas.getContext('2d')!;
		context.drawImage(bitmap, 0, 0);
		const corner = context.getImageData(bitmap.width - 1, 0, 1, 1).data[3];
		return {
			width: bitmap.width,
			height: bitmap.height,
			bytes: blob.size,
			corner,
			type: blob.type
		};
	}, address);
}

test('pictures are made small before they leave the browser', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#account');
	const avatar = `/api/profiles/${USER}/images/avatar`;
	const banner = `/api/profiles/${USER}/images/banner`;
	const sent: number[] = [];
	page.on('request', (r) => {
		if (r.method() === 'PUT' && r.url().includes('/account/images/')) {
			sent.push(r.postDataBuffer()?.length ?? 0);
		}
	});

	const photo = await pictureFile(page, 4000, 3000, 'image/png');
	await page.getByRole('button', { name: 'Edit banner' }).click();
	await page
		.locator('.file', { hasText: 'Upload a picture' })
		.locator('input')
		.setInputFiles({ name: 'huge.png', mimeType: 'image/png', buffer: photo });
	await expect(page.getByText('Banner updated')).toBeVisible();
	const onServer = await readPicture(page, banner);
	expect(onServer.width).toBe(1800);
	expect(onServer.height).toBe(1350);
	expect(sent[0]).toBeLessThan(photo.length);
	expect(onServer.corner, 'a clear corner stays clear').toBe(0);

	await page
		.locator('.file', { hasText: 'Picture' })
		.locator('input')
		.setInputFiles({ name: 'photo.png', mimeType: 'image/png', buffer: photo });
	await page.getByRole('button', { name: 'Use this picture' }).click();
	await expect(page.getByText('Profile picture updated')).toBeVisible();
	const framed = await readPicture(page, avatar);
	expect([framed.width, framed.height]).toEqual([512, 512]);
	expect(framed.corner, 'a clear corner stays clear').toBe(0);

	// One frame is enough to see the bytes arrive untouched.
	const gif = Buffer.from(
		'R0lGODlhAQABAIAAAAAAAP///yH/C05FVFNDQVBFMi4wAwEAAAAh+QQJCgAAACwAAAAAAQABAAACAkQBACH5BAkKAAAALAAAAAABAAEAAAICRAEAOw==',
		'base64'
	);
	await page
		.locator('.file', { hasText: 'Picture' })
		.locator('input')
		.setInputFiles({ name: 'moving.gif', mimeType: 'image/gif', buffer: gif });
	await expect(page.getByText('Profile picture updated').last()).toBeVisible();
	await expect
		.poll(async () => (await page.request.get(`${avatar}?v=gif`)).headers()['content-type'])
		.toBe('image/gif');
	const stored = await (await page.request.get(`${avatar}?v=gif2`)).body();
	expect(stored.equals(gif), 'an animated GIF is stored as it was sent').toBe(true);

	// A moving PNG reads the same as a still one until its bytes are looked at.
	const chunk = (type: string, length: number) =>
		Buffer.concat([
			Buffer.from([0, 0, 0, length]),
			Buffer.from(type),
			Buffer.alloc(length),
			Buffer.alloc(4)
		]);
	const moving = Buffer.concat([
		Buffer.from('89504e470d0a1a0a', 'hex'),
		chunk('IHDR', 13),
		chunk('acTL', 8),
		chunk('IDAT', 1)
	]);
	await page
		.locator('.file', { hasText: 'Picture' })
		.locator('input')
		.setInputFiles({ name: 'moving.png', mimeType: 'image/png', buffer: moving });
	await expect
		.poll(async () => (await (await page.request.get(`${avatar}?v=png`)).body()).length)
		.toBe(moving.length);
	const kept = await (await page.request.get(`${avatar}?v=png2`)).body();
	expect(kept.equals(moving), 'an APNG is stored as it was sent').toBe(true);

	await page
		.locator('.file', { hasText: 'Picture' })
		.locator('input')
		.setInputFiles({
			name: 'broken.png',
			mimeType: 'image/png',
			buffer: Buffer.from('not a picture')
		});
	await expect(page.getByText('That picture could not be read.')).toBeVisible();

	await page.request.delete('/api/account/images/avatar');
	await page.request.delete('/api/account/images/banner');
});

test('Home shows what your people watched, and a profile how alike you rate', async ({
	page,
	playwright,
	baseURL
}, testInfo) => {
	await signIn(page);
	const name = `pal${Date.now() % 100000}`;
	const invite = await (await page.request.post('/api/invites', { data: { role: 'user' } })).json();
	const friend = await playwright.request.newContext({ baseURL });
	await friend.post('/api/auth/register', {
		data: { username: name, password: 'another-long-pass', invite: invite.code, remember: true }
	});
	await friend.put('/api/account/profile', { data: { public: true, display_name: 'Mira' } });

	const db = new DatabaseSync(join('.e2e-data', testInfo.project.name, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const id = (username: string) =>
		(db.prepare('SELECT id FROM app_user WHERE username = ?').get(username) as { id: number }).id;
	const [me, them] = [id(USER), id(name)];
	const now = '2026-01-01T00:00:00Z';
	const secret = 'Only for my own eyes';
	for (const title of [9130, 9131, 9132, 9133, 9134]) {
		db.prepare(
			`INSERT OR IGNORE INTO title (id, kind, name, genres_json, refreshed_at) VALUES (?, 'movie', ?, '[]', ?)`
		).run(title, `Seen ${title}`, now);
		db.prepare(
			`INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
			 VALUES (?, 'tmdb', ?, 1, ?)`
		).run(title, `movie/${title}`, now);
		for (const [account, rating] of [
			[me, 80],
			[them, 70]
		]) {
			db.prepare(
				`INSERT OR REPLACE INTO library_entry (user_id, title_id, status, rating, notes, added_at, updated_at)
				 VALUES (?, ?, 'completed', ?, ?, ?, ?)`
			).run(account, title, rating, secret, now, now);
		}
	}
	db.prepare(`INSERT INTO watch_event (user_id, title_id, watched_at) VALUES (?, 9130, ?)`).run(
		them,
		new Date(Date.now() - 3_600_000).toISOString()
	);
	db.close();

	const seen = ['shelves', 'continue', 'thisweek', 'activity', 'because', 'recent', 'favourites'];
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', {
		data: { ...saved, homeRows: ['activity', 'continue'], homeRowsSeen: seen }
	});

	await page.goto('/');
	const row = page.locator('.rail', { has: page.getByRole('heading', { name: 'Activity' }) });
	await expect(row.getByText('watched Seen 9130')).toBeVisible();
	await expect(row.getByText('Mira')).toBeVisible();
	await expect(page.getByText(secret)).toHaveCount(0);

	await page.goto(`/people/${name}`);
	await expect(page.getByText('80% taste match')).toBeVisible();
	await expect(page.getByText(secret)).toHaveCount(0);
	await page.goto(`/people/${USER}`);
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(page.getByText('taste match')).toHaveCount(0);

	// A row with its profile off is gone, and takes the row with it.
	await friend.put('/api/account/profile', { data: { public: false } });
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Activity' })).toHaveCount(0);
	await friend.put('/api/account/profile', { data: { public: true } });

	// Switched off in Settings, and nothing is asked for. The Home page before
	// still had the row and may still be asking for it; leave it first, so only
	// the new page's requests are counted.
	await page.request.put('/api/settings', {
		data: { ...saved, homeRows: ['continue'], homeRowsSeen: seen }
	});
	await page.goto('about:blank');
	const asked: string[] = [];
	page.on('request', (r) => asked.push(new URL(r.url()).pathname));
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Activity' })).toHaveCount(0);
	await page.waitForTimeout(300);
	expect(asked).not.toContain('/api/home/activity');
	await page.request.put('/api/settings', { data: saved });
	await friend.dispose();
});

/** Long names with nowhere to break, the worst a person can type or a provider can send. */
const LONG = {
	username: 'w'.repeat(50),
	friend: 'q'.repeat(50),
	display: 'D'.repeat(40),
	shelf: 'S'.repeat(60),
	title: 'T'.repeat(90)
};

/** An administrator with long names everywhere, written where the API would refuse them. */
async function longAccount(
	project: string,
	playwright: import('@playwright/test').PlaywrightWorkerArgs['playwright'],
	baseURL: string,
	owner: Page
) {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	// A retry starts on the same database.
	for (const name of [LONG.username, LONG.friend]) {
		db.prepare('DELETE FROM app_user WHERE username = ?').run(name);
	}
	const member = async (role: string, temporary: string, name: string) => {
		const made = await (await owner.request.post('/api/invites', { data: { role } })).json();
		const context = await playwright.request.newContext({ baseURL });
		await context.post('/api/auth/register', {
			data: {
				username: temporary,
				password: 'another-long-pass',
				invite: made.code,
				remember: true
			}
		});
		await context.put('/api/account/profile', {
			data: { public: true, display_name: LONG.display, bio: 'B'.repeat(600) }
		});
		db.prepare('UPDATE app_user SET username = ? WHERE username = ?').run(name, temporary);
		return context;
	};
	const stamp = Date.now() % 100000;
	const me = await member('admin', `long${stamp}`, LONG.username);
	const friend = await member('user', `pal${stamp}`, LONG.friend);
	await friend.dispose();

	const made = await me.post('/api/shelves', { data: { name: LONG.shelf, kind: 'manual' } });
	const { shelf } = await made.json();

	const now = '2026-01-01T00:00:00Z';
	const { id: user } = db
		.prepare('SELECT id FROM app_user WHERE username = ?')
		.get(LONG.username) as { id: number };
	const { id: pal } = db.prepare('SELECT id FROM app_user WHERE username = ?').get(LONG.friend) as {
		id: number;
	};
	db.prepare(
		`INSERT OR IGNORE INTO title (id, kind, name, genres_json, refreshed_at) VALUES (9120, 'tv', ?, '["Drama"]', ?)`
	).run(LONG.title, now);
	db.prepare(
		`INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
		 VALUES (9120, 'tmdb', 'tv/9120', 1, ?)`
	).run(now);
	const day = (offset: number) => new Date(Date.now() + offset * 86_400_000).toISOString();
	const episode = (number: number, airs: string) =>
		db
			.prepare(
				`INSERT OR REPLACE INTO episode (title_id, season_number, episode_number, name, air_date)
				 VALUES (9120, 1, ?, ?, ?) RETURNING id`
			)
			.get(number, 'E'.repeat(90), airs.slice(0, 10)) as { id: number };
	const { id: watched } = episode(1, day(-3));
	episode(2, day(-1));
	episode(3, day(2));
	for (const [account, status] of [
		[user, 'watching'],
		[pal, 'completed']
	]) {
		db.prepare(
			`INSERT OR REPLACE INTO library_entry (user_id, title_id, status, favourite, added_at, updated_at)
			 VALUES (?, 9120, ?, 1, ?, ?)`
		).run(account, status, now, now);
		db.prepare(
			`INSERT INTO watch_event (user_id, title_id, episode_id, watched_at) VALUES (?, 9120, ?, ?)`
		).run(account, watched, day(-1));
	}
	db.prepare('INSERT OR IGNORE INTO shelf_item (shelf_id, title_id) VALUES (?, 9120)').run(
		shelf.id
	);
	db.close();

	const state = await me.storageState();
	await me.dispose();
	return { state, shelf: shelf.id as number };
}

/** A page has drawn what it fetched once its skeletons are gone. */
async function settled(page: Page) {
	await page.waitForSelector('main > *');
	await page.waitForFunction(() => !document.querySelector('[aria-busy="true"]'));
	await page.waitForTimeout(350);
}

/** What on this page has gone past the edge of the screen. */
function pastTheEdge(page: Page, selector?: string) {
	return page.evaluate((selector) => {
		const width = document.documentElement.clientWidth;
		const out: string[] = [];
		const widest = Math.max(document.documentElement.scrollWidth, document.body.scrollWidth);
		if (widest > width) {
			const clipped = (element: Element) => {
				for (let at = element.parentElement; at; at = at.parentElement) {
					if (/auto|scroll|hidden|clip/.test(getComputedStyle(at).overflowX)) return true;
				}
				return false;
			};
			const culprits = [...document.querySelectorAll('body *')]
				.filter((e) => e.getBoundingClientRect().right > width + 1 && !clipped(e))
				.slice(-3)
				.map((e) => `${e.tagName.toLowerCase()}.${String(e.className).split(' ')[0]}`);
			out.push(`scrolls sideways: ${widest} in ${width} (${culprits.join(', ')})`);
		}
		for (const name of ['Search and commands', 'Account']) {
			const button = document.querySelector(`[aria-label^="${name}"]`);
			if (!button) continue;
			const box = button.getBoundingClientRect();
			if (box.right > width || box.left < 0) out.push(`${name} is off the screen`);
		}
		if (selector) {
			const element = document.querySelector(selector);
			if (!element) out.push(`${selector} is not there`);
			else {
				const box = element.getBoundingClientRect();
				if (box.right > width + 0.5 || box.left < -0.5) {
					out.push(`${selector} runs ${Math.round(box.left)} to ${Math.round(box.right)}`);
				}
				for (const child of element.querySelectorAll('*')) {
					const inner = child.getBoundingClientRect();
					if (inner.width && (inner.right > width + 0.5 || inner.left < -0.5)) {
						out.push(`${selector} ${child.tagName}.${child.className} past the edge`);
						break;
					}
				}
			}
		}
		return out;
	}, selector);
}

test('a very long name never widens a page on a phone', async ({
	page,
	browser,
	playwright,
	baseURL
}, testInfo) => {
	test.setTimeout(240_000);
	await signIn(page);
	const { state, shelf } = await longAccount(
		testInfo.project.name,
		playwright,
		baseURL ?? '',
		page
	);

	const paths = [
		'/',
		'/welcome',
		'/people',
		`/people/${LONG.username}`,
		`/people/${LONG.username}/library`,
		`/people/${LONG.friend}`,
		`/people/${LONG.friend}/library`,
		'/settings#account',
		'/settings#appearance',
		'/settings#home',
		'/settings#watching',
		'/settings#connections',
		'/settings#data',
		'/admin',
		'/admin/database',
		'/admin/database/app_user',
		'/about',
		'/shelves',
		`/shelves/${shelf}`,
		'/library',
		'/t/9120',
		`/search?q=${LONG.title}`,
		`/search?q=${LONG.username}`,
		'/stats',
		'/calendar?view=diary',
		'/calendar',
		'/airing',
		'/import',
		'/whats-new'
	];

	const problems: string[] = [];
	for (const width of [360, 320]) {
		const context = await browser.newContext({
			storageState: state,
			baseURL,
			viewport: { width, height: 740 }
		});
		const seen = await enforceDeployedCsp(context, baseURL ?? '');
		const tab = await context.newPage();
		tab.setDefaultTimeout(5000);
		const note = (at: string, found: string[]) =>
			problems.push(...found.map((f) => `${width}px ${at}: ${f}`));

		for (const path of paths) {
			await tab.goto(path);
			await settled(tab);
			note(path, await pastTheEdge(tab));
		}

		await tab.goto('/about');
		await settled(tab);
		await tab.getByRole('button', { name: 'Account' }).click();
		note('account menu', await pastTheEdge(tab, '[role="menu"]'));
		await tab.keyboard.press('Escape');
		await tab.goto('/whats-new');
		await settled(tab);

		await tab.getByRole('button', { name: 'More' }).click();
		note('More sheet', await pastTheEdge(tab, '.more-sheet'));
		await tab.getByRole('button', { name: 'More' }).click();

		await tab.goto(`/shelves/${shelf}`);
		await settled(tab);
		await tab.getByRole('button', { name: 'Edit' }).click();
		await tab.getByRole('button', { name: `Remove ${LONG.title}` }).click();
		await tab.getByRole('button', { name: 'Undo' }).waitFor();
		note('undo toast', await pastTheEdge(tab, '.stack'));
		await tab.getByRole('button', { name: 'Undo' }).click();
		await tab.getByRole('button', { name: `Remove ${LONG.title}` }).waitFor();

		await tab.goto('/shelves');
		await settled(tab);
		await tab.getByRole('button', { name: 'New shelf' }).click();
		await tab.getByRole('dialog').getByRole('textbox').first().fill(LONG.shelf);
		note('new shelf dialog', await pastTheEdge(tab, '[role="dialog"]'));
		await tab.keyboard.press('Escape');

		await tab.goto('/library');
		await settled(tab);
		await tab.getByRole('button', { name: 'Filters', exact: false }).first().click();
		note('library filters', await pastTheEdge(tab, '[role="dialog"]'));
		await tab.keyboard.press('Escape');

		await tab.goto('/');
		await settled(tab);
		await tab.getByRole('button', { name: 'Search and commands' }).click();
		await tab.keyboard.type(LONG.title.slice(0, 30));
		await tab.waitForTimeout(600);
		note('search palette', await pastTheEdge(tab, '.palette'));

		expect(seen, 'blocked by the deployed Content-Security-Policy').toEqual([]);
		await context.close();
	}
	expect(problems).toEqual([]);
});

/** Titles Frog already has, out of anyone's library: there is no TMDB key here. */
function seedTitles(project: string, titles: [number, 'movie' | 'tv', string][]) {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const now = '2026-01-01T00:00:00Z';
	for (const [id, kind, name] of titles) {
		db.prepare(
			`INSERT OR IGNORE INTO title (id, kind, name, genres_json, release_date, refreshed_at)
			 VALUES (?, ?, ?, '[]', '2020-01-01', ?)`
		).run(id, kind, name, now);
		db.prepare(
			`INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
			 VALUES (?, 'tmdb', ?, 1, ?)`
		).run(id, `${kind}/${id}`, now);
		db.prepare('DELETE FROM library_entry WHERE title_id = ?').run(id);
		db.prepare('DELETE FROM watch_event WHERE title_id = ?').run(id);
	}
	db.close();
}

/** Where a title stands in the signed-in account's library, straight from the database. */
function statusOf(project: string, titleId: number): string | null {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const row = db
		.prepare(
			`SELECT l.status FROM library_entry l JOIN app_user u ON u.id = l.user_id
			  WHERE u.username = ? AND l.title_id = ?`
		)
		.get(USER, titleId) as { status: string } | undefined;
	db.close();
	return row?.status ?? null;
}

/** Search answering with these titles, which Frog already has. */
async function searchAnswers(page: Page, titles: [number, 'movie' | 'tv', string][]) {
	const results = titles.map(([id, kind, name]) => ({
		kind,
		tmdb_id: id,
		id,
		name,
		poster_path: null,
		backdrop_path: null,
		year: 2020,
		overview: null,
		vote: null,
		popularity: 1,
		vote_count: 1,
		genre_ids: [],
		in_library: null
	}));
	await page.route('**/api/search?*', (route) =>
		route.fulfill({ json: { results, next_page: null } })
	);
	await page.route('**/api/search/more?*', (route) =>
		route.fulfill({ json: { people: [], characters: [], studios: [] } })
	);
}

test('the + asks for a status, and adds nothing when closed', async ({ page }, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	const titles: [number, 'movie' | 'tv', string][] = [
		[9201, 'tv', 'Plus Show'],
		[9202, 'movie', 'Plus Film']
	];
	seedTitles(project, titles);
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', { data: { ...saved, addStatus: 'ask' } });
	await searchAnswers(page, titles);

	await page.goto('/search?q=plus');
	const plus = page.getByRole('button', { name: 'Add Plus Show to your library' });
	await plus.click();
	const menu = page.getByRole('menu');
	await expect(menu).toBeVisible();
	// The Watchlist is focused, so Enter alone would take it.
	await expect(menu.getByRole('menuitem', { name: 'Watchlist' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);
	expect(statusOf(project, 9201)).toBeNull();

	await plus.click();
	await page.getByRole('menuitem', { name: 'Watching', exact: true }).click();
	await expect(page.getByText('Added Plus Show to Watching')).toBeVisible();
	await expect.poll(() => statusOf(project, 9201)).toBe('watching');

	// Undo takes it back out.
	await page.getByRole('button', { name: 'Undo' }).click();
	await expect.poll(() => statusOf(project, 9201)).toBeNull();

	await page.request.put('/api/settings', { data: saved });
	seedTitles(project, titles);
});

test('with a status chosen in Settings, the + adds in one tap', async ({ page }, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	const titles: [number, 'movie' | 'tv', string][] = [[9203, 'tv', 'One Tap Show']];
	seedTitles(project, titles);
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', { data: { ...saved, addStatus: 'planned' } });
	await searchAnswers(page, titles);

	await page.goto('/search?q=tap');
	await page.getByRole('button', { name: 'Add One Tap Show to your library' }).click();
	await expect(page.getByText('Added One Tap Show to Watchlist')).toBeVisible();
	await expect(page.getByRole('menu')).toHaveCount(0);
	await expect.poll(() => statusOf(project, 9203)).toBe('planned');

	await page.request.put('/api/settings', { data: saved });
	seedTitles(project, titles);
});

test('an old default status carries over only when it was not the Watchlist', async ({ page }) => {
	await signIn(page);
	const saved = await (await page.request.get('/api/settings')).json();
	const { addStatus: _gone, ...older } = saved;
	const shown = () =>
		page
			.locator('.row', { hasText: 'Adding a title' })
			.getByRole('button', { name: /Adding a title/ });

	await page.request.put('/api/settings', { data: { ...older, defaultStatus: 'paused' } });
	await page.goto('/settings#watching');
	await expect(shown()).toContainText('On hold');

	await page.request.put('/api/settings', { data: { ...older, defaultStatus: 'planned' } });
	// The same address again only scrolls: reload, so the settings are read anew.
	await page.reload();
	await expect(shown()).toContainText('Ask each time');

	await page.request.put('/api/settings', { data: saved });
});

/** Another account on this Frog, signed in, with its profile on and the welcome read. */
async function anotherAccount(
	page: Page,
	playwright: { request: import('@playwright/test').APIRequest },
	baseURL: string | undefined,
	prefix: string
) {
	const name = `${prefix}${Date.now() % 100000}`;
	const invite = await (await page.request.post('/api/invites', { data: { role: 'user' } })).json();
	const them = await playwright.request.newContext({ baseURL });
	await them.post('/api/auth/register', {
		data: { username: name, password: 'another-long-pass', invite: invite.code, remember: true }
	});
	await them.put('/api/account/profile', { data: { public: true } });
	const settings = await (await them.get('/api/settings')).json();
	await them.put('/api/settings', { data: { ...settings, onboarded: true } });
	return { name, them };
}

test('a recommendation goes to the one person, who adds it from Notifications', async ({
	page,
	browser,
	playwright,
	baseURL
}, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	seedTitles(project, [[9401, 'tv', 'Rec Show']]);
	const { name, them } = await anotherAccount(page, playwright, baseURL, 'rec');
	await page.route('**/api/t/9401', (route) =>
		route.fulfill({ json: titleAnswer(9401, 'tv', 'Rec Show', {}) })
	);

	await page.goto('/t/9401');
	await page.getByRole('button', { name: 'Recommend' }).click();
	const dialog = page.getByRole('dialog', { name: 'Recommend Rec Show' });
	await expect(dialog).toBeVisible();
	// Nobody ticked yet: nothing to send.
	await expect(dialog.getByRole('button', { name: 'Send' })).toBeDisabled();
	await dialog.getByText(name, { exact: true }).click();
	// The note is out of the way until asked for.
	await expect(dialog.getByRole('textbox')).toHaveCount(0);
	await dialog.getByRole('button', { name: 'Add a note' }).click();
	await dialog.getByRole('textbox').fill('Season two is the one');
	await dialog.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Recommended Rec Show')).toBeVisible();

	// Only they have it.
	const mine = await (await page.request.get('/api/notifications')).json();
	expect(mine.for_you.map((r: { title: { name: string } }) => r.title.name)).not.toContain(
		'Rec Show'
	);

	const context = await browser.newContext({ baseURL, storageState: await them.storageState() });
	const theirs = await context.newPage();
	await theirs.goto('/notifications');
	await expect(theirs.getByRole('heading', { name: 'Notifications' })).toBeVisible();
	const card = theirs.locator('.rec', { hasText: 'Rec Show' });
	await expect(card).toBeVisible();
	await expect(card).toContainText('Season two is the one');
	// Seen is read: the bell has nothing left to say.
	await expect(theirs.getByRole('link', { name: 'Notifications', exact: true })).toBeVisible();

	await card.getByRole('button', { name: 'Add' }).click();
	await theirs.getByRole('menuitem', { name: 'Watching', exact: true }).click();
	await expect(theirs.getByText('Added Rec Show to Watching')).toBeVisible();
	await expect(card).toContainText('Watching');

	// The sender hears, quietly, that it was added.
	await expect
		.poll(async () => {
			const r = await (await page.request.get('/api/notifications')).json();
			return r.items.some((i: { kind: string }) => i.kind === 'added');
		})
		.toBe(true);
	await page.goto('/notifications');
	await expect(page.getByText(new RegExp(`${name} added Rec Show`))).toBeVisible();

	await context.close();
	await them.put('/api/account/profile', { data: { public: false } });
	await them.dispose();
});

test('the bell counts what is new and hides at nothing', async ({
	page,
	playwright,
	baseURL
}, testInfo) => {
	await signIn(page);
	seedTitles(testInfo.project.name, [[9402, 'movie', 'Bell Film']]);
	const { them } = await anotherAccount(page, playwright, baseURL, 'bell');
	await page.request.post('/api/notifications/read', { data: { ids: [] } });
	await page.goto('/search');
	await expect(page.getByRole('link', { name: 'Notifications', exact: true })).toBeVisible();

	// Someone recommends something: on the next look, the bell says so.
	const account = await (await page.request.get('/api/account')).json();
	await page.request.put('/api/account/profile', { data: { public: true } });
	const people = await (await them.get('/api/people/choosable?for=recommend')).json();
	const mihai = people.people.find((p: { username: string }) => p.username === USER);
	expect(mihai, 'the owner can be chosen').toBeTruthy();
	await them.post('/api/t/9402/recommend', { data: { to: [mihai.id] } });
	await page.reload();
	await expect(page.getByRole('link', { name: 'Notifications, 1 new' })).toBeVisible();

	await page.getByRole('link', { name: 'Notifications, 1 new' }).click();
	await expect(page).toHaveURL('/notifications');
	await expect(page.getByRole('link', { name: 'Notifications', exact: true })).toBeVisible();
	await page.request.put('/api/account/profile', { data: { public: account.profile_public } });
	await them.put('/api/account/profile', { data: { public: false } });
	await them.dispose();
});

test('a notification can be removed, and Clear all takes the rest, each with Undo', async ({
	page,
	playwright,
	baseURL,
	isMobile
}, testInfo) => {
	await signIn(page);
	seedTitles(testInfo.project.name, [
		[9403, 'movie', 'Gone Film'],
		[9404, 'movie', 'Kept Film']
	]);
	const { them } = await anotherAccount(page, playwright, baseURL, 'rm');
	const account = await (await page.request.get('/api/account')).json();
	await page.request.put('/api/account/profile', { data: { public: true } });
	const people = await (await them.get('/api/people/choosable?for=recommend')).json();
	const me = people.people.find((p: { username: string }) => p.username === USER);
	await them.post('/api/t/9403/recommend', { data: { to: [me.id] } });
	await them.post('/api/t/9404/recommend', { data: { to: [me.id] } });

	await page.goto('/notifications');
	await page.getByRole('button', { name: 'Everything' }).click();
	const line = page.locator('.notices li', { hasText: 'Gone Film' });
	await expect(line).toBeVisible();
	if (isMobile) {
		// Swiped away by finger.
		const box = (await line.boundingBox())!;
		const y = box.y + box.height / 2;
		const touch = { pointerType: 'touch', pointerId: 7, isPrimary: true, clientY: y };
		await line.dispatchEvent('pointerdown', { ...touch, clientX: box.x + box.width - 20 });
		for (const x of [box.x + box.width - 60, box.x + box.width / 2, box.x + 20]) {
			await line.dispatchEvent('pointermove', { ...touch, clientX: x });
		}
		await line.dispatchEvent('pointerup', { ...touch, clientX: box.x + 20 });
	} else {
		await line.hover();
		await line.getByRole('button', { name: /^Remove notification/ }).click();
	}
	await expect(line).toHaveCount(0);
	await expect(page.getByText('Notification removed')).toBeVisible();

	// Only the line went: the recommendation can still be answered.
	const after = await (await page.request.get('/api/notifications')).json();
	expect(after.items.some((i: { title?: { name: string } }) => i.title?.name === 'Gone Film')).toBe(
		false
	);
	expect(after.for_you.some((r: { title: { name: string } }) => r.title.name === 'Gone Film')).toBe(
		true
	);

	await page.getByRole('button', { name: 'Undo' }).click();
	await expect(page.locator('.notices li', { hasText: 'Gone Film' })).toBeVisible();
	// The toast used is on its way out.
	await expect(page.getByText('Notification removed')).toHaveCount(0);

	const cleared = page.waitForResponse(
		(r) => r.request().method() === 'DELETE' && r.url().endsWith('/api/notifications')
	);
	await page.getByRole('button', { name: 'Clear all' }).click();
	await cleared;
	await expect(page.locator('.notices li')).toHaveCount(0);
	expect((await (await page.request.get('/api/notifications')).json()).items).toHaveLength(0);
	await page
		.locator('.toast', { hasText: 'Notifications cleared' })
		.getByRole('button', { name: 'Undo' })
		.click();
	await expect(page.locator('.notices li', { hasText: 'Kept Film' })).toBeVisible();

	await page.request.put('/api/account/profile', { data: { public: account.profile_public } });
	await them.put('/api/account/profile', { data: { public: false } });
	await them.dispose();
});

test('Home’s hero takes turns: recommendations, trending, what airs this week', async ({
	page
}) => {
	await signIn(page);
	const item = (id: number, kind: 'movie' | 'tv', name: string) =>
		titleAnswer(id, kind, name, {}).item;
	const sam = { username: 'sam', display_name: 'Sam', avatar: false };
	const today = new Date().toISOString().slice(0, 10);
	let home = {
		hero: { item: item(9601, 'tv', 'Continuing Show'), next: null },
		hero_row: 'continue',
		recommended: [
			{
				id: 71,
				note: 'The second season is the one',
				created_at: today,
				from: sam,
				item: item(9602, 'movie', 'Recommended Film')
			},
			{ id: 72, note: null, created_at: today, from: sam, item: item(9603, 'tv', 'Second Pick') }
		],
		continue: [],
		waiting: [],
		this_week: [
			{
				item: item(9604, 'tv', 'Airing Show'),
				air_date: today,
				episodes: [
					{ season_number: 2, episode_number: 3, name: 'The Third', air_date: today, air_at: null }
				]
			}
		],
		recent: [],
		favourites: [],
		shelves: []
	};
	await page.route('**/api/home', (route) => route.fulfill({ json: home }));
	await page.route('**/api/home/featured', (route) =>
		route.fulfill({ json: { items: [{ item: item(9605, 'movie', 'Trending Film'), rank: 2 }] } })
	);
	const answered: unknown[] = [];
	await page.route('**/api/recommendations/*/answer', (route) => {
		answered.push(route.request().postDataJSON());
		return route.fulfill({ json: { ok: true } });
	});
	await page.goto('/');

	const hero = page.getByRole('region', { name: 'Featured' });
	await expect(hero).toBeVisible();
	const dots = hero.getByRole('group', { name: 'Choose a slide' }).getByRole('button');
	// One of each kind in turn, so a long list of one never pushes the
	// others out: recommendation, trending, this week, recommendation.
	await expect(dots).toHaveCount(4);
	await expect(dots.nth(0)).toHaveAccessibleName(/Recommended Film/);
	await expect(dots.nth(1)).toHaveAccessibleName(/Trending Film/);
	await expect(dots.nth(2)).toHaveAccessibleName(/Airing Show/);
	await expect(dots.nth(3)).toHaveAccessibleName(/Second Pick/);
	// A recommendation, with who sent it and their note.
	await expect(hero).toContainText('Recommended by Sam');
	await expect(hero).toContainText('The second season is the one');
	// Never what you are in the middle of; that goes back in its row.
	await expect(hero).not.toContainText('Continuing Show');
	await expect(page.getByRole('heading', { name: 'Continue watching' })).toBeVisible();
	await expect(page.locator('main')).toContainText('Continuing Show');

	// It turns by itself, about every eight seconds: trending, with its place.
	await expect(hero).toContainText('Trending Film', { timeout: 11000 });
	await expect(hero).toContainText('#2');
	await expect(hero).toContainText('Trending this week');
	await expect(hero.getByRole('link', { name: 'More info' })).toBeVisible();
	await dots.nth(2).click();
	await expect(hero).toContainText('Airing Show');
	await expect(hero).toContainText('S02E03');
	await expect(hero.getByRole('link', { name: 'View show' })).toBeVisible();
	await dots.nth(3).click();
	await expect(hero).toContainText('Second Pick');

	// Declining answers it and takes it out of the rotation.
	await dots.nth(0).click();
	await expect(hero).toContainText('Recommended Film');
	home = { ...home, recommended: home.recommended.slice(1) };
	// The slide shown, not the one fading out behind it.
	await hero
		.getByRole('group', { name: '1 of 4' })
		.getByRole('button', { name: 'Not for me' })
		.click();
	await expect(dots).toHaveCount(3);
	expect(answered).toEqual([{ answer: 'dismissed' }]);
	await expect(hero).not.toContainText('Recommended Film');
});

test("a studio's titles go in together, and come out together", async ({ page }, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	const seeded: [number, 'movie' | 'tv', string][] = [
		[9501, 'movie', 'Studio One'],
		[9502, 'movie', 'Studio Two'],
		[9503, 'tv', 'Studio Three'],
		[9504, 'movie', 'Studio Held']
	];
	seedTitles(project, seeded);
	// One already in the library, which stays exactly as it is.
	await page.request.post('/api/library/bulk', {
		data: { action: 'add', status: 'paused', title_ids: [9504] }
	});
	const titles = seeded.map(([id, kind, name]) => ({
		kind,
		tmdb_id: id,
		// As TMDB lists them: by its ids only.
		id: id === 9504 ? 9504 : null,
		name,
		poster_path: null,
		backdrop_path: null,
		year: 2020,
		overview: null,
		vote: null,
		popularity: 1,
		vote_count: 1,
		genre_ids: [],
		in_library: id === 9504 ? 'paused' : null
	}));
	await page.route('**/api/studios/7*', (route) =>
		route.fulfill({ json: { id: 7, name: 'A Studio', titles } })
	);

	await page.goto('/studio/7');
	await page.getByRole('button', { name: 'Select' }).click();
	const bar = page.getByRole('toolbar', { name: 'Choose titles' });
	await expect(bar).toContainText('0 selected');
	// What is held cannot be ticked.
	await page.getByRole('checkbox', { name: /Studio Held/ }).click({ force: true });
	await expect(bar).toContainText('0 selected');
	await bar.getByRole('button', { name: 'Select all' }).click();
	await expect(bar).toContainText('3 selected');

	await bar.getByRole('button', { name: 'Add 3' }).click();
	await page.getByRole('menuitem', { name: 'Watching', exact: true }).click();
	await expect(page.getByText('Added 3 titles to Watching')).toBeVisible();
	for (const id of [9501, 9502, 9503]) {
		await expect.poll(() => statusOf(project, id)).toBe('watching');
	}
	expect(statusOf(project, 9504)).toBe('paused');

	await page.getByRole('button', { name: 'Undo' }).click();
	for (const id of [9501, 9502, 9503]) {
		await expect.poll(() => statusOf(project, id)).toBeNull();
	}
	expect(statusOf(project, 9504)).toBe('paused');
	seedTitles(project, seeded);
});

/** Finished, unrated titles in the signed-in account's library, for the rating deck. */
function rateDeck(project: string, titles: [number, 'movie' | 'tv', string][]) {
	seedTitles(project, titles);
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const { id: user } = db.prepare('SELECT id FROM app_user WHERE username = ?').get(USER) as {
		id: number;
	};
	// Each finished a day apart, newest first, so the deck deals them in this order.
	titles.forEach(([id], i) => {
		const day = `2026-0${1 + Math.floor(i / 9)}-${String(28 - i).padStart(2, '0')}T20:00:00Z`;
		db.prepare(
			`INSERT OR REPLACE INTO library_entry (user_id, title_id, status, added_at, updated_at)
			 VALUES (?, ?, 'completed', ?, ?)`
		).run(user, id, day, day);
	});
	db.close();
}

function ratingOf(project: string, titleId: number): number | null {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const row = db
		.prepare(
			`SELECT l.rating FROM library_entry l JOIN app_user u ON u.id = l.user_id
			  WHERE u.username = ? AND l.title_id = ?`
		)
		.get(USER, titleId) as { rating: number | null } | undefined;
	db.close();
	return row?.rating ?? null;
}

const FAST: [number, 'movie' | 'tv', string][] = [
	[9601, 'movie', 'Fast One'],
	[9602, 'movie', 'Fast Two'],
	[9603, 'movie', 'Fast Three'],
	[9604, 'movie', 'Fast Four'],
	[9605, 'movie', 'Fast Five']
];

test('ratings given as fast as the keys go are every one saved, on the right card', async ({
	page
}, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	rateDeck(project, FAST);
	const restore = await smileys(page);
	await page.goto('/rate');
	await expect(page.locator('.card.top h2')).toHaveText('Fast One');
	// 1, 2, 3, 1, 2: no waiting for a card to leave.
	for (const key of ['1', '2', '3', '1', '2']) await page.keyboard.press(key);
	const want = [33, 67, 100, 33, 67];
	for (const [i, [id]] of FAST.entries()) {
		await expect.poll(() => Math.round(ratingOf(project, id) ?? -1)).toBe(want[i]);
	}
	await expect(page.getByText('5 rated')).toBeVisible();
	await restore();
	seedTitles(project, FAST);
});

test('a rating the server refuses brings its card back', async ({ page }, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	rateDeck(project, FAST.slice(0, 2));
	const restore = await smileys(page);
	await page.route('**/api/t/9601/library', (route) =>
		route.request().method() === 'PUT' ? route.fulfill({ status: 500, json: {} }) : route.continue()
	);
	await page.goto('/rate');
	const top = page.locator('.card.top h2');
	await expect(top).toHaveText('Fast One');
	await expect(page.getByText('2 to go')).toBeVisible();
	await page.keyboard.press('3');
	await expect(top).toHaveText('Fast One');
	await expect(page.getByText('2 to go')).toBeVisible();
	await expect(page.getByText(/rated/)).toHaveCount(0);
	expect(ratingOf(project, 9601)).toBeNull();
	await restore();
	seedTitles(project, FAST);
});

test('a card can leave the library, or the watched pile, and come back with Undo', async ({
	page
}, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	rateDeck(project, FAST.slice(0, 3));
	await page.goto('/rate');
	const top = page.locator('.card.top h2');
	await expect(top).toHaveText('Fast One');

	// Never watched: back to the Watchlist, and out of the deck.
	await page.getByRole('button', { name: 'Fix this title' }).click();
	await page.getByRole('menuitem', { name: 'Not watched yet' }).click();
	await expect(top).toHaveText('Fast Two');
	await expect.poll(() => statusOf(project, 9601)).toBe('planned');

	// Removed: gone from the library.
	await page.getByRole('button', { name: 'Fix this title' }).click();
	await page.getByRole('menuitem', { name: 'Remove from library' }).click();
	await expect(top).toHaveText('Fast Three');
	await expect.poll(() => statusOf(project, 9602)).toBeNull();

	// The page's Undo takes both back, newest first.
	const undo = page.locator('.moves').getByRole('button', { name: 'Undo' });
	await undo.click();
	await expect(top).toHaveText('Fast Two');
	await expect.poll(() => statusOf(project, 9602)).toBe('completed');
	await undo.click();
	await expect(top).toHaveText('Fast One');
	await expect.poll(() => statusOf(project, 9601)).toBe('completed');

	// A status changed on the card stays on the card.
	await page.getByRole('button', { name: 'Fix this title' }).click();
	await page.getByRole('menuitem', { name: 'Change status' }).click();
	await page.getByRole('menuitemcheckbox', { name: 'On hold' }).click();
	await expect.poll(() => statusOf(project, 9601)).toBe('paused');
	await expect(top).toHaveText('Fast One');
	await expect(page.locator('.card.top .facts')).toContainText('On hold');

	// And the count starts again with another deck.
	await page.keyboard.press('ArrowRight');
	await page.getByRole('button', { name: 'Everything', exact: true }).click();
	await expect(page.getByText(/rated/)).toHaveCount(0);
	seedTitles(project, FAST);
});

function watchesOf(project: string, username: string, titleId: number): number {
	const db = new DatabaseSync(join('.e2e-data', project, 'e2e.db'));
	db.exec('PRAGMA busy_timeout = 5000');
	const row = db
		.prepare(
			`SELECT COUNT(*) AS n FROM watch_event w JOIN app_user u ON u.id = w.user_id
			  WHERE u.username = ? AND w.title_id = ?`
		)
		.get(username, titleId) as { n: number };
	db.close();
	return row.n;
}

test('a film watched with someone reaches them, and they add it to their own diary', async ({
	page,
	browser,
	playwright,
	baseURL
}, testInfo) => {
	await signIn(page);
	const project = testInfo.project.name;
	const films: [number, 'movie' | 'tv', string][] = [
		[9701, 'movie', 'With Film'],
		[9702, 'movie', 'Alone Film']
	];
	seedTitles(project, films);
	const { name, them } = await anotherAccount(page, playwright, baseURL, 'with');
	const saved = await (await page.request.get('/api/settings')).json();
	await page.request.put('/api/settings', { data: { ...saved, addStatus: 'ask' } });
	await searchAnswers(page, films);
	await page.goto('/search?q=film');

	// Without opening the row, Completed is still one click away.
	await page.getByRole('button', { name: 'Add Alone Film to your library' }).click();
	await page.getByRole('menuitem', { name: 'Completed', exact: true }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog.getByRole('button', { name: 'Watched with…' })).toBeVisible();
	await dialog.getByRole('button', { name: 'Mark watched' }).click();
	await expect(page.getByText('Added Alone Film to Completed')).toBeVisible();
	expect(watchesOf(project, USER, 9702)).toBe(1);

	await page.getByRole('button', { name: 'Add With Film to your library' }).click();
	await page.getByRole('menuitem', { name: 'Completed', exact: true }).click();
	await dialog.getByRole('button', { name: 'Watched with…' }).click();
	await dialog.getByText(name, { exact: true }).click();
	await dialog.getByRole('button', { name: 'Mark watched' }).click();
	await expect(page.getByText('Added With Film to Completed')).toBeVisible();
	// Nothing written for them.
	expect(watchesOf(project, name, 9701)).toBe(0);

	const context = await browser.newContext({ baseURL, storageState: await them.storageState() });
	const theirs = await context.newPage();
	await theirs.goto('/notifications');
	const line = theirs.locator('li', { hasText: 'With Film' });
	await expect(line).toContainText(`${USER} watched With Film with you`);
	await line.getByRole('button', { name: 'Add to my diary' }).click();
	await theirs.getByRole('menuitem', { name: 'Completed', exact: true }).click();
	await expect(theirs.getByText('Added to your diary')).toBeVisible();
	await expect.poll(() => watchesOf(project, name, 9701)).toBe(1);

	await context.close();
	await page.request.put('/api/settings', { data: saved });
	await them.put('/api/account/profile', { data: { public: false } });
	await them.dispose();
	seedTitles(project, films);
});

async function signIn(page: import('@playwright/test').Page) {
	await page.goto('/login');
	if (!page.url().includes('/login')) return;

	const heading = await page.getByRole('heading').first().textContent();
	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page
		.getByRole('button', { name: heading?.includes('Set up') ? 'Create account' : 'Sign in' })
		.click();
	// A brand-new account is shown the welcome first. Read past it, as a
	// person would, so everything after this starts on Home.
	await expect(page).toHaveURL(/\/(welcome)?$/);
	if (page.url().includes('/welcome')) {
		await page.getByRole('button', { name: 'Skip' }).click();
		await expect(page).toHaveURL('/');
	}
}
