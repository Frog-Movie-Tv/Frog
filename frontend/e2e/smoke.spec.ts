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

	// A new account with nothing tracked is welcomed first. Home shows for a
	// moment before it sends you on, so wait for the welcome itself; on a
	// retry the account may have been welcomed already and Home stays.
	const welcome = page.getByRole('heading', { name: /^Welcome/ });
	const welcomed = await welcome.waitFor({ timeout: 10000 }).then(
		() => true,
		() => false
	);
	if (welcomed) {
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
		['/diary', /^Diary · Frog$/],
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
		await expect(page.locator('.bar kbd')).toHaveText('Ctrl K');
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
	await expect(page.getByRole('heading', { name: 'Diary', level: 1 })).toBeVisible();
	await expect(page.getByText('Nothing watched yet')).toBeVisible();
});

test('the about page credits its sources', async ({ page }) => {
	await signIn(page);
	await page.goto('/about');

	await expect(page.getByRole('heading', { name: 'Frog', level: 1 })).toBeVisible();
	await expect(page.getByText('Bullfrog')).toBeVisible();
	await expect(page.getByRole('link', { name: /TMDB/ }).first()).toBeVisible();
	await expect(page.getByRole('link', { name: /Lucide/ })).toBeVisible();
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

	// Pond is the default now.
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'pond');

	await page.getByRole('button', { name: /AMOLED/ }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'amoled');

	// The choice must survive a reload, or it is not a setting.
	await page.reload();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'amoled');

	await page.getByRole('button', { name: /^Pond$/ }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'pond');
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

	// An unrated control has to say so, not show a dash.
	await page.getByRole('button', { name: 'Rated 7.8' }).click();
	await page.getByRole('menuitemradio', { name: '7.5' }).click();
	await expect(page.getByRole('button', { name: 'Rated 7.5' })).toBeVisible();
});

test('a saved colour scheme can be copied again and again', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings#appearance');
	await page.getByText('Colour editor').click();
	await page.getByLabel('Accent').fill('#e06c9f');
	await page.getByRole('button', { name: 'Save', exact: true }).click();

	// Two copies of one theme used to share a name, and the page fell over.
	const duplicate = page.getByRole('button', { name: 'Duplicate' }).first();
	await duplicate.click();
	await duplicate.click();
	await expect(page.getByText('My theme copy 2')).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Theme', exact: true })).toBeVisible();

	await page.reload();
	await expect(page.getByRole('heading', { name: 'Theme', exact: true })).toBeVisible();

	// Leave the account as it was for the tests after this one.
	const res = await page.request.get('/api/settings');
	const saved = await res.json();
	await page.request.put('/api/settings', {
		data: { ...saved, savedThemes: [], customTheme: null }
	});
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
	await expect(titles(page)).toHaveCount(3);
	await expect(titles(page).first()).toHaveAttribute('title', 'Alien');
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

	await expect(page.locator('html')).toHaveAttribute('data-theme', 'pond');
	const theirs = await (await page.request.get('/api/settings')).json();
	expect(theirs.theme ?? 'pond').toBe('pond');

	// Back to the first account, as it was.
	await page.request.post('/api/auth/logout', { data: {} });
	await signIn(page);
	const mine = await (await page.request.get('/api/settings')).json();
	expect(mine.theme).toBe('light');
	await page.request.put('/api/settings', { data: { ...mine, theme: 'pond' } });
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
		'/api/diary',
		'/api/export/library.json',
		'/api/auth/two-factor',
		'/api/stats',
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
		'/api/account',
		'/api/account/sessions',
		'/api/admin/users',
		'/api/admin/instance',
		'/api/admin/server',
		'/api/profiles',
		'/api/profiles/someone',
		'/api/profiles/someone/images/avatar',
		'/api/anilist',
		'/api/anilist/entries',
		'/api/invites',
		'/api/search/more?q=frog',
		'/api/search/genres',
		'/api/t/1/scores',
		'/api/studios/1',
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
		'/api/push/test',
		'/api/shelves',
		'/api/shelves/1/items',
		'/api/invites',
		'/api/anilist/connect',
		'/api/anilist/sync',
		'/api/t/1/episodes/watch',
		'/api/t/1/episodes/1/watch',
		'/api/t/1/episodes/1/1/watch'
	]) {
		const res = await request.post(path, { data: {} });
		expect(res.status(), `POST ${path} should be 401`).toBe(401);
	}
	for (const path of [
		'/api/t/1/seasons/1/rating',
		'/api/t/1/library',
		'/api/t/1/episodes/1/1/state',
		'/api/anilist/entries/1'
	]) {
		const res = await request.put(path, { data: { rating: 80 } });
		expect(res.status(), `PUT ${path} should be 401`).toBe(401);
	}
	for (const path of ['/api/account', '/api/account/library', '/api/t/1/library']) {
		const res = await request.delete(path, { data: {} });
		expect(res.status(), `DELETE ${path} should be 401`).toBe(401);
	}

	// Joining is open by design, so it cannot be behind the wall. Without a
	// live invite it still refuses, which is what keeps sign-up closed.
	const join = await request.post('/api/auth/register', {
		data: { username: 'nobody', password: 'a-long-enough-pass', invite: 'made-up' }
	});
	expect(join.status(), 'registering without a real invite is refused').toBe(403);
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
