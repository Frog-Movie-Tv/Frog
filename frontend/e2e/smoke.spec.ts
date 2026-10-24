import { expect, test } from '@playwright/test';

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

test('a cold instance sends you to first-run setup', async ({ page }) => {
	await page.goto('/');
	await expect(page).toHaveURL(/\/login/);
	await expect(page.getByRole('heading', { name: 'Set up Frog' })).toBeVisible();
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

	await expect(page).toHaveURL('/');
	await expect(page.getByText('Nothing here yet')).toBeVisible();
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

test('the stats page loads instead of showing a database error', async ({ page }) => {
	await signIn(page);
	await page.goto('/stats');

	await expect(page.getByRole('heading', { name: 'Stats', level: 1 })).toBeVisible();
	await expect(page.getByText('Titles tracked')).toBeVisible();
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

test("the what's new page lists the current release", async ({ page }) => {
	await signIn(page);
	await page.goto('/whats-new');

	await expect(page.getByRole('heading', { name: "What's new" })).toBeVisible();
	await expect(page.getByText('Installed')).toBeVisible();
	await expect(page.getByText('Tadpole')).toBeVisible();

	// Release notes must not be cut off at the first line wrap, and <kbd>
	// must render as a key rather than as literal angle brackets.
	const bullet = page.locator('li').filter({ hasText: /Import from Letterboxd, Trakt, IMDb/ }).first();
	await expect(bullet).toContainText('you decide whether the file or your library wins');

	const shortcut = page.locator('li').filter({ hasText: /command palette on/i }).first();
	await expect(shortcut.locator('kbd').first()).toBeVisible();
	await expect(page.getByText('<kbd>')).toHaveCount(0);
});

test('the importer offers every supported source and both exports', async ({ page }) => {
	await signIn(page);
	await page.goto('/import');

	for (const source of ['Letterboxd', 'IMDb', 'Trakt', 'TV Time', 'Frog backup']) {
		await expect(page.getByRole('button', { name: source, exact: true })).toBeVisible();
	}
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
	await expect(page.getByText('Tadpole')).toBeVisible();
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
	await expect(page.getByPlaceholder('Films, shows, anything on TMDB')).toBeVisible();
	await expect(page.getByText('Search is unavailable until metadata is connected')).toBeVisible();
});

test('settings shows the instance facts and switches theme', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings');

	await expect(page.getByText(USER)).toBeVisible();
	await expect(page.getByText('missing')).toBeVisible();

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
	await page.goto('/settings');

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
	await fresh.goto('/settings');
	const stored = await fresh.request.get('/api/settings');
	expect((await stored.json()).mascot).toBe('sleepy');
	await fresh.close();
});

test('score format changes how a rating reads without touching the value', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings');

	await page.getByRole('button', { name: /100 point/ }).click();
	await expect(page.getByRole('button', { name: 'Rated 78' })).toBeVisible();

	await page.getByRole('button', { name: /10 point decimal/ }).click();
	await expect(page.getByRole('button', { name: 'Rated 7.8' })).toBeVisible();

	// An unrated control has to say so, not show a dash.
	await page.getByRole('button', { name: 'Rated 7.8' }).click();
	await page.getByRole('menuitemradio', { name: '7.5' }).click();
	await expect(page.getByRole('button', { name: 'Rated 7.5' })).toBeVisible();
});

test('signing out closes the door behind you', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings');
	await page.getByRole('button', { name: 'Sign out' }).click();

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
		'/img/w342/nope.jpg'
	]) {
		const res = await request.get(path);
		expect(res.status(), `${path} should be 401`).toBe(401);
	}
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
	await expect(page).toHaveURL('/');
}
