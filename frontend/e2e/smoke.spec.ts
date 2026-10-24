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
	await expect(page.getByRole('heading', { name: 'Library', level: 1 })).toBeVisible();
});

test('an empty library explains itself instead of showing a blank grid', async ({ page }) => {
	await signIn(page);
	await expect(page.getByText('Your library is empty')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Find something to watch' })).toBeVisible();
});

test('search says plainly that no TMDB key is configured', async ({ page }) => {
	await signIn(page);
	await page.goto('/search');
	await expect(page.getByPlaceholder('Films, shows, anything on TMDB')).toBeVisible();
	await expect(page.getByText('No TMDB key is set')).toBeVisible();
});

test('settings shows the instance facts and switches theme', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings');

	await expect(page.getByText(USER)).toBeVisible();
	await expect(page.getByText('missing')).toBeVisible();

	await page.getByRole('button', { name: /AMOLED/ }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'amoled');

	// The choice must survive a reload, or it is not a setting.
	await page.reload();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'amoled');

	await page.getByRole('button', { name: /Dark/ }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('score format changes how a rating reads without touching the value', async ({ page }) => {
	await signIn(page);
	await page.goto('/settings');

	await page.getByRole('button', { name: /100 point/ }).click();
	await expect(page.getByRole('status').or(page.locator('output'))).toContainText('78');

	await page.getByRole('button', { name: /10 point decimal/ }).click();
	await expect(page.locator('output')).toContainText('7.8');

	await page.getByRole('button', { name: /5 stars/ }).click();
	await expect(page.getByRole('group', { name: 'Rating' })).toBeVisible();
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
	for (const path of ['/api/library', '/api/library/stats', '/img/w342/nope.jpg']) {
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
