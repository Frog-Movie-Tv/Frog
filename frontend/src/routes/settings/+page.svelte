<script lang="ts">
	import { api } from '#lib/api';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import { goto } from '$app/navigation';
	import Logo from '#lib/components/Logo.svelte';
	import Rating from '#lib/components/Rating.svelte';
	import Select from '#lib/components/Select.svelte';
	import Notifications from '#lib/components/Notifications.svelte';
	import AccountSettings from '#lib/components/AccountSettings.svelte';
	import AniListSync from '#lib/components/AniListSync.svelte';
	import TwoFactor from '#lib/components/TwoFactor.svelte';
	import { SCORE_FORMATS, type ScoreFormat } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import {
		DEFAULTS,
		FONTS,
		HOME_ROWS,
		MAX_TABS,
		NAV_PAGES,
		MASCOTS,
		THEMES,
		settings,
		type AnimationLevel,
		type CardStyle,
		type CustomTheme,
		type FontName,
		type HeroStyle,
		type MascotName,
		type ThemeName
	} from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { STATUSES, STATUS_LABEL, type Status } from '#lib/types';

	let preview = $state<number | null>(78);
	let themeName = $state('My theme');
	let importing = $state(false);

	const s = $derived(settings.current);

	const tokens = [
		{ key: 'bg', label: 'Background' },
		{ key: 'bg-elev', label: 'Surface' },
		{ key: 'bg-elev-2', label: 'Raised surface' },
		{ key: 'accent', label: 'Accent' },
		{ key: 'text', label: 'Text' }
	] as const;

	/** Read the colour actually in force, so the pickers start where you are. */
	function currentToken(key: string): string {
		const override = s.customTheme?.tokens?.[key as keyof CustomTheme['tokens']];
		if (override) return override;
		if (typeof document === 'undefined') return '#000000';
		return toHex(getComputedStyle(document.documentElement).getPropertyValue(`--${key}`).trim());
	}

	/** Colour inputs only accept #rrggbb, so normalise whatever the token holds. */
	function toHex(value: string): string {
		if (value.startsWith('#')) {
			return value.length === 4
				? '#' + [...value.slice(1)].map((c) => c + c).join('')
				: value.slice(0, 7);
		}
		const m = value.match(/rgba?\(([^)]+)\)/);
		if (!m) return '#000000';
		const [r, g, b] = m[1].split(',').map((n) => Math.round(parseFloat(n)));
		return '#' + [r, g, b].map((n) => n.toString(16).padStart(2, '0')).join('');
	}

	function setToken(key: string, value: string) {
		const base: CustomTheme = s.customTheme ?? { name: themeName, base: s.theme, tokens: {} };
		settings.set('customTheme', {
			...base,
			base: s.theme,
			tokens: { ...base.tokens, [key]: value }
		});
	}

	function saveTheme() {
		if (!s.customTheme) {
			toasts.push('Change a colour first', 'error');
			return;
		}
		const theme: CustomTheme = { ...s.customTheme, name: themeName.trim() || 'My theme' };
		const rest = s.savedThemes.filter((t) => t.name !== theme.name);
		settings.patch({ savedThemes: [...rest, theme], customTheme: theme });
		toasts.push(`Saved “${theme.name}”`);
	}

	function applyTheme(theme: CustomTheme) {
		settings.patch({ theme: theme.base, customTheme: theme });
		themeName = theme.name;
	}

	function duplicateTheme(theme: CustomTheme) {
		const copy = { ...theme, name: `${theme.name} copy` };
		settings.patch({ savedThemes: [...s.savedThemes, copy], customTheme: copy });
		themeName = copy.name;
	}

	function download(name: string, data: unknown) {
		const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = name;
		a.click();
		URL.revokeObjectURL(url);
	}

	async function importJson(file: File | null | undefined, kind: 'theme' | 'all') {
		if (!file) return;
		importing = true;
		try {
			const parsed = JSON.parse(await file.text());
			if (kind === 'theme') {
				if (!parsed || typeof parsed !== 'object' || !parsed.tokens) {
					throw new Error('That file is not a Frog theme');
				}
				const theme = parsed as CustomTheme;
				settings.patch({
					savedThemes: [...s.savedThemes.filter((t) => t.name !== theme.name), theme],
					customTheme: theme,
					theme: theme.base ?? s.theme
				});
				toasts.push(`Imported “${theme.name}”`);
			} else {
				settings.replace({ ...DEFAULTS, ...parsed });
				toasts.push('Settings restored');
			}
		} catch (e) {
			toasts.error(e);
		} finally {
			importing = false;
		}
	}

	function moveRow(id: string, delta: number) {
		const rows = [...s.homeRows];
		const i = rows.indexOf(id);
		const j = i + delta;
		if (i < 0 || j < 0 || j >= rows.length) return;
		[rows[i], rows[j]] = [rows[j], rows[i]];
		settings.set('homeRows', rows);
	}

	function toggleRow(id: string) {
		settings.set(
			'homeRows',
			s.homeRows.includes(id) ? s.homeRows.filter((r) => r !== id) : [...s.homeRows, id]
		);
	}

	async function signOut() {
		await session.logout();
		toasts.push('Signed out');
		await goto('/login', { replaceState: true });
	}

	const navPages = $derived.by(() => {
		const order = s.navOrder;
		const known = order
			.map((href) => NAV_PAGES.find((p) => p.href === href))
			.filter((p): p is (typeof NAV_PAGES)[number] => !!p);
		return [...known, ...NAV_PAGES.filter((p) => !order.includes(p.href))];
	});

	function moveNav(href: string, delta: number) {
		const order = navPages.map((p) => p.href);
		const i = order.indexOf(href);
		const j = i + delta;
		if (j < 0 || j >= order.length) return;
		[order[i], order[j]] = [order[j], order[i]];
		settings.set('navOrder', order);
	}

	function toggleHidden(href: string, hide: boolean) {
		settings.set(
			'navHidden',
			hide ? [...s.navHidden, href] : s.navHidden.filter((h) => h !== href)
		);
	}

	function toggleTab(href: string, on: boolean) {
		if (on && s.navTabs.length >= MAX_TABS) return;
		settings.set('navTabs', on ? [...s.navTabs, href] : s.navTabs.filter((h) => h !== href));
	}

	const hiddenRows = $derived(HOME_ROWS.filter((r) => !s.homeRows.includes(r.id)));

	/** Names for shelf rows ("shelf:12"), which are not in HOME_ROWS. */
	let shelfNames = $state<Record<string, string>>({});
	$effect(() => {
		void api
			.shelves()
			.then(
				(r) => (shelfNames = Object.fromEntries(r.shelves.map((x) => [`shelf:${x.id}`, x.name])))
			)
			.catch(() => undefined);
	});
	function rowInfo(id: string): { label: string; hint: string } | undefined {
		const fixed = HOME_ROWS.find((r) => r.id === id);
		if (fixed) return fixed;
		if (id.startsWith('shelf:') && shelfNames[id]) return { label: shelfNames[id], hint: 'Shelf' };
		return undefined;
	}

	let update = $state<{
		state: 'idle' | 'checking' | 'current' | 'available' | 'failed';
		tag?: string;
		url?: string;
	}>({
		state: 'idle'
	});

	/**
	 * Deliberately a button rather than something automatic: Frog otherwise
	 * talks to nobody but TMDB, and a silent call home would undercut that.
	 */
	async function checkForUpdates() {
		update = { state: 'checking' };
		try {
			const res = await fetch('https://api.github.com/repos/Frog-Movie-Tv/Frog/releases/latest', {
				headers: { accept: 'application/vnd.github+json' }
			});
			if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
			const release = await res.json();
			const tag = String(release.tag_name ?? '').replace(/^v/, '');
			const here = session.status?.version ?? '';
			update =
				tag && tag !== here
					? { state: 'available', tag, url: release.html_url }
					: { state: 'current', tag };
		} catch (e) {
			update = { state: 'failed' };
			toasts.error(e);
		}
	}
	const cardToggles = [
		['showBadge', 'Status badge'],
		['showRating', 'My rating'],
		['showProgress', 'Progress bar'],
		['showYear', 'Year']
	] as const;
</script>

<svelte:head>
	<title>Settings · Frog</title>
</svelte:head>

<div class="page">
	<h1 class="title">Settings</h1>

	<section class="block">
		<div class="section-head"><h2>Account</h2></div>
		<AccountSettings />
	</section>

	<section class="block">
		<div class="section-head"><h2>Theme</h2></div>

		<div class="tiles">
			{#each THEMES as t (t.id)}
				<button
					class="tile"
					class:on={s.theme === t.id}
					aria-label={t.label}
					aria-pressed={s.theme === t.id}
					onclick={() => settings.set('theme', t.id as ThemeName)}
				>
					<span class="swatches" aria-hidden="true">
						{#each t.swatches as c (c)}<i style:background={c}></i>{/each}
					</span>
					<strong>{t.label}</strong>
					<span class="faint">{t.hint}</span>
				</button>
			{/each}
		</div>

		<details class="editor">
			<summary>Colour editor</summary>

			<div class="colours">
				{#each tokens as token (token.key)}
					<label class="colour">
						<input
							type="color"
							value={currentToken(token.key)}
							oninput={(e) => setToken(token.key, e.currentTarget.value)}
						/>
						<span>{token.label}</span>
					</label>
				{/each}
			</div>

			<div class="actions">
				<input class="input name" bind:value={themeName} aria-label="Theme name" />
				<button class="btn" onclick={saveTheme}>Save</button>
				<button class="btn ghost" onclick={() => settings.set('customTheme', null)}>
					Reset colours
				</button>
				<button
					class="btn ghost"
					disabled={!s.customTheme}
					onclick={() =>
						download(
							`${(s.customTheme?.name ?? 'frog-theme').replace(/\s+/g, '-')}.json`,
							s.customTheme
						)}
				>
					Export
				</button>
				<label class="btn ghost file">
					Import
					<input
						type="file"
						accept="application/json"
						disabled={importing}
						onchange={(e) => importJson(e.currentTarget.files?.[0], 'theme')}
					/>
				</label>
			</div>

			{#if s.savedThemes.length}
				<ul class="saved">
					{#each s.savedThemes as theme (theme.name)}
						<li>
							<span class="dots" aria-hidden="true">
								{#each Object.values(theme.tokens) as c (c)}<i style:background={c}></i>{/each}
							</span>
							<strong>{theme.name}</strong>
							<button class="link" onclick={() => applyTheme(theme)}>Use</button>
							<button class="link" onclick={() => duplicateTheme(theme)}>Duplicate</button>
							<button
								class="link danger"
								onclick={() =>
									settings.set(
										'savedThemes',
										s.savedThemes.filter((t) => t.name !== theme.name)
									)}
							>
								Delete
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</details>
	</section>

	<section class="block">
		<div class="section-head"><h2>Mascot</h2></div>

		<div class="tiles mascots">
			{#each MASCOTS as m (m.id)}
				<button
					class="tile"
					class:on={s.mascot === m.id}
					aria-label={m.label}
					aria-pressed={s.mascot === m.id}
					onclick={() => settings.set('mascot', m.id as MascotName)}
				>
					<img src="/mascots/{m.id}.svg" alt="" width="44" height="44" />
					<strong>{m.label}</strong>
				</button>
			{/each}
		</div>
		<p class="faint tiny">Reinstall the app to change its home-screen icon.</p>
	</section>

	<section class="block">
		<div class="section-head"><h2>Type and shape</h2></div>

		<div class="rows">
			<div class="row card">
				<span class="label"><strong>Font</strong></span>
				<Select
					label="Font"
					value={s.font}
					options={FONTS.map((f) => ({ value: f.id, label: f.label }))}
					onchange={(v) => settings.set('font', v as FontName)}
				/>
			</div>

			<label class="row card">
				<span class="label">
					<strong>Text size</strong><span class="faint">{Math.round(s.textScale * 100)}%</span>
				</span>
				<input
					type="range"
					min="0.85"
					max="1.3"
					step="0.05"
					value={s.textScale}
					oninput={(e) => settings.set('textScale', Number(e.currentTarget.value))}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Corner radius</strong><span class="faint">{Math.round(16 * s.radiusScale)}px</span
					>
				</span>
				<input
					type="range"
					min="0"
					max="2"
					step="0.125"
					value={s.radiusScale}
					oninput={(e) => settings.set('radiusScale', Number(e.currentTarget.value))}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Glass blur</strong>
					<span class="faint">{s.blur === 0 ? 'Off — solid bars' : `${s.blur}px`}</span>
				</span>
				<input
					type="range"
					min="0"
					max="26"
					step="2"
					value={s.blur}
					oninput={(e) => settings.set('blur', Number(e.currentTarget.value))}
				/>
			</label>

			<div class="row card">
				<span class="label">
					<strong>Animation</strong>
				</span>
				<Select
					label="Animation"
					value={s.animations}
					options={[
						{ value: 'full', label: 'Full' },
						{ value: 'reduced', label: 'Reduced' },
						{ value: 'off', label: 'Off' }
					]}
					onchange={(v) => settings.set('animations', v as AnimationLevel)}
				/>
			</div>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Cards and grid</h2></div>

		<div class="rows">
			<div class="row card">
				<span class="label">
					<strong>Card style</strong>
				</span>
				<Select
					label="Card style"
					value={s.cardStyle}
					options={[
						{ value: 'poster', label: 'Poster' },
						{ value: 'landscape', label: 'Landscape' },
						{ value: 'list', label: 'List' },
						{ value: 'compact', label: 'Compact table' }
					]}
					onchange={(v) => settings.set('cardStyle', v as CardStyle)}
				/>
			</div>

			<label class="row card">
				<span class="label">
					<strong>Poster size</strong><span class="faint">{s.posterSize}px columns</span>
				</span>
				<input
					type="range"
					min="90"
					max="230"
					step="2"
					value={s.posterSize}
					oninput={(e) => settings.set('posterSize', Number(e.currentTarget.value))}
				/>
			</label>

			<label class="row card">
				<span class="label"
					><strong>Grid spacing</strong><span class="faint">{s.gridGap}px</span></span
				>
				<input
					type="range"
					min="6"
					max="34"
					step="2"
					value={s.gridGap}
					oninput={(e) => settings.set('gridGap', Number(e.currentTarget.value))}
				/>
			</label>

			{#each cardToggles as [key, label] (key)}
				<label class="row card">
					<span class="label"><strong>{label}</strong></span>
					<input
						type="checkbox"
						checked={s[key]}
						onchange={(e) => settings.set(key, e.currentTarget.checked)}
					/>
				</label>
			{/each}
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Hero and pages</h2></div>

		<div class="rows">
			<div class="row card">
				<span class="label">
					<strong>Hero</strong>
				</span>
				<Select
					label="Hero"
					value={s.heroStyle}
					options={[
						{ value: 'backdrop', label: 'Backdrop' },
						{ value: 'poster', label: 'Poster' },
						{ value: 'off', label: 'Off' }
					]}
					onchange={(v) => settings.set('heroStyle', v as HeroStyle)}
				/>
			</div>

			<label class="row card">
				<span class="label">
					<strong>Follow the poster</strong>
					<span class="faint">Colour each title page from its artwork</span>
				</span>
				<input
					type="checkbox"
					checked={s.followPoster}
					onchange={(e) => settings.set('followPoster', e.currentTarget.checked)}
				/>
			</label>

			<div class="row card">
				<span class="label">
					<strong>Open on</strong>
				</span>
				<Select
					label="Open on"
					value={s.landingPage}
					options={[
						{ value: '/', label: 'Home' },
						{ value: '/library', label: 'Library' },
						{ value: '/search', label: 'Search' },
						{ value: '/calendar', label: 'Calendar' },
						{ value: '/diary', label: 'Diary' },
						{ value: '/stats', label: 'Stats' }
					]}
					onchange={(v) => settings.set('landingPage', v)}
				/>
			</div>
		</div>

		<h3 class="sub">Home rows</h3>
		<ul class="builder">
			{#each s.homeRows as id, i (id)}
				{@const row = rowInfo(id)}
				{#if row}
					<li>
						<span class="grip"><Icon name="grip" size={16} /></span>
						<span class="label">
							<strong>{row.label}</strong><span class="faint">{row.hint}</span>
						</span>
						<button
							class="icon-btn"
							disabled={i === 0}
							aria-label="Move up"
							onclick={() => moveRow(id, -1)}
						>
							<Icon name="chevron-up" size={15} />
						</button>
						<button
							class="icon-btn"
							disabled={i === s.homeRows.length - 1}
							aria-label="Move down"
							onclick={() => moveRow(id, 1)}
						>
							<Icon name="chevron-down" size={15} />
						</button>
						<button class="icon-btn" aria-label="Remove row" onclick={() => toggleRow(id)}>
							<Icon name="x" size={15} />
						</button>
					</li>
				{/if}
			{/each}
		</ul>

		{#if hiddenRows.length}
			<div class="actions">
				{#each hiddenRows as row (row.id)}
					<button class="btn ghost small" onclick={() => toggleRow(row.id)}>
						<Icon name="plus" size={14} />
						{row.label}
					</button>
				{/each}
			</div>
		{/if}
	</section>

	<section class="block">
		<div class="section-head"><h2>Ratings</h2></div>

		<div class="tiles small">
			{#each SCORE_FORMATS as f (f.id)}
				<button
					class="tile"
					class:on={s.scoreFormat === f.id}
					aria-label={f.label}
					aria-pressed={s.scoreFormat === f.id}
					onclick={() => settings.set('scoreFormat', f.id as ScoreFormat)}
				>
					<strong>{f.label}</strong>
					<span class="faint">{f.example}</span>
				</button>
			{/each}
		</div>

		<div class="rows">
			<label class="row card">
				<span class="label">
					<strong>Half stars</strong>
				</span>
				<input
					type="checkbox"
					checked={s.halfStars}
					onchange={(e) => settings.set('halfStars', e.currentTarget.checked)}
				/>
			</label>

			<div class="row card">
				<span class="label"><strong>Preview</strong></span>
				<Rating value={preview} onchange={(v) => (preview = v)} />
			</div>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Watching</h2></div>

		<div class="rows">
			<div class="row card">
				<span class="label">
					<strong>Default status</strong>
				</span>
				<Select
					label="Default status"
					value={s.defaultStatus}
					options={STATUSES.map((st) => ({ value: st, label: STATUS_LABEL[st] }))}
					onchange={(v) => settings.set('defaultStatus', v as Status)}
				/>
			</div>

			<label class="row card">
				<span class="label">
					<strong>Spoiler protection</strong>
					<span class="faint">Blur unwatched episode stills</span>
				</span>
				<input
					type="checkbox"
					checked={s.spoilerProtection}
					onchange={(e) => settings.set('spoilerProtection', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Hide animation</strong>
					<span class="faint">Leave animated titles out of search</span>
				</span>
				<input
					type="checkbox"
					checked={s.hideAnimation}
					onchange={(e) => settings.set('hideAnimation', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Automatic dates</strong>
				</span>
				<input
					type="checkbox"
					checked={s.autoDates}
					onchange={(e) => settings.set('autoDates', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Complete automatically</strong>
					<span class="faint">When the last episode is ticked</span>
				</span>
				<input
					type="checkbox"
					checked={s.autoComplete}
					onchange={(e) => settings.set('autoComplete', e.currentTarget.checked)}
				/>
			</label>

			<div class="row card">
				<span class="label"><strong>Dates</strong></span>
				<Select
					label="Dates"
					value={s.dateStyle}
					options={[
						{ value: 'friendly', label: 'Friendly (3 days ago)' },
						{ value: 'long', label: 'Long (24 October 2026)' },
						{ value: 'iso', label: 'ISO (2026-10-24)' }
					]}
					onchange={(v) => settings.set('dateStyle', v as 'friendly')}
				/>
			</div>

			<label class="row card">
				<span class="label"><strong>24-hour clock</strong></span>
				<input
					type="checkbox"
					checked={s.clock24}
					onchange={(e) => settings.set('clock24', e.currentTarget.checked)}
				/>
			</label>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Notifications</h2></div>
		<Notifications />
	</section>

	<section class="block">
		<div class="section-head"><h2>Security</h2></div>
		<TwoFactor />
	</section>

	<section class="block">
		<div class="section-head"><h2>Your data</h2></div>

		<div class="rows">
			<a class="row card link-row" href="/import">
				<span class="label">
					<strong>Import</strong>
					<span class="faint"
						>Letterboxd, Trakt, IMDb, AniList, MyAnimeList, TV Time or a Frog backup</span
					>
				</span>
				<Icon name="chevron-right" size={18} />
			</a>
			<AniListSync />
			<a class="row card link-row" href="/api/export/library.json" download>
				<span class="label">
					<strong>Download backup</strong>
					<span class="faint">Everything, as JSON</span>
				</span>
				<Icon name="download" size={18} />
			</a>
			<a class="row card link-row" href="/api/export/library.csv" download>
				<span class="label">
					<strong>Download spreadsheet</strong>
					<span class="faint">Your library, as CSV</span>
				</span>
				<Icon name="download" size={18} />
			</a>
			<label class="row card">
				<span class="label">
					<strong>Data saver</strong>
					<span class="faint">Smaller artwork everywhere</span>
				</span>
				<input
					type="checkbox"
					checked={s.dataSaver}
					onchange={(e) => settings.set('dataSaver', e.currentTarget.checked)}
				/>
			</label>
		</div>
	</section>

	<section class="block">
		<div class="section-head">
			<h2>Navigation</h2>
			<span class="faint">{s.navTabs.length} of {MAX_TABS} in the phone tab bar</span>
		</div>

		<ul class="builder">
			{#each navPages as navPage, i (navPage.href)}
				{@const inBar = !s.navHidden.includes(navPage.href)}
				{@const inTabs = s.navTabs.includes(navPage.href)}
				<li>
					<span class="nav-icon"><Icon name={navPage.icon as IconName} size={17} /></span>
					<span class="label"><strong>{navPage.label}</strong></span>
					<label class="mini" title="Show in the menu bar">
						<input
							type="checkbox"
							checked={inBar || navPage.fixed}
							disabled={navPage.fixed}
							onchange={(e) => toggleHidden(navPage.href, !e.currentTarget.checked)}
						/>
						Menu
					</label>
					<label class="mini" title="Show in the phone tab bar">
						<input
							type="checkbox"
							checked={inTabs}
							disabled={!inTabs && s.navTabs.length >= MAX_TABS}
							onchange={(e) => toggleTab(navPage.href, e.currentTarget.checked)}
						/>
						Tab
					</label>
					<button
						class="icon-btn"
						disabled={i === 0}
						aria-label="Move {navPage.label} up"
						onclick={() => moveNav(navPage.href, -1)}
					>
						<Icon name="chevron-up" size={15} />
					</button>
					<button
						class="icon-btn"
						disabled={i === navPages.length - 1}
						aria-label="Move {navPage.label} down"
						onclick={() => moveNav(navPage.href, 1)}
					>
						<Icon name="chevron-down" size={15} />
					</button>
				</li>
			{/each}
		</ul>
	</section>

	<section class="block">
		<div class="section-head"><h2>Advanced</h2></div>

		<details class="editor">
			<summary>Custom CSS</summary>
			<p class="faint tiny">
				Applied to every page, for example <code>:root &#123; --accent: hotpink; &#125;</code>
			</p>
			<textarea
				class="input code"
				rows="7"
				spellcheck="false"
				placeholder=":root &#123; --radius: 4px; &#125;"
				value={s.customCss}
				onchange={(e) => settings.set('customCss', e.currentTarget.value)}></textarea>
		</details>

		<div class="actions">
			<button class="btn" onclick={() => download('frog-settings.json', s)}>
				Export all settings
			</button>
			<label class="btn ghost file">
				Import settings
				<input
					type="file"
					accept="application/json"
					disabled={importing}
					onchange={(e) => importJson(e.currentTarget.files?.[0], 'all')}
				/>
			</label>
			<button class="btn ghost" onclick={() => settings.reset()}>Reset everything</button>
		</div>
	</section>

	<section class="block" id="this-instance">
		<div class="section-head"><h2>This instance</h2></div>

		<div class="instance card">
			<Logo size={38} />
			<dl class="facts">
				<div>
					<dt>Signed in as</dt>
					<dd>{session.status?.username ?? '—'}</dd>
				</div>
				<div>
					<dt>Version</dt>
					<dd>{session.status?.version ?? '—'}</dd>
				</div>
				<div>
					<dt>Provider region</dt>
					<dd>{session.status?.region ?? '—'}</dd>
				</div>
				<div>
					<dt>TMDB key</dt>
					<dd class={session.status?.tmdb_configured ? 'ok' : 'bad'}>
						{session.status?.tmdb_configured ? 'configured' : 'missing'}
					</dd>
				</div>
			</dl>
		</div>

		<div class="actions">
			<a class="btn" href="/whats-new">Release notes</a>
			<a class="btn ghost" href="/about">About</a>
			<button class="btn ghost" onclick={checkForUpdates} disabled={update.state === 'checking'}>
				{update.state === 'checking' ? 'Checking…' : 'Check for updates'}
			</button>
			<button class="btn ghost" onclick={signOut}>Sign out</button>
		</div>

		{#if update.state === 'available'}
			<p class="update available">
				<strong>Version {update.tag} is out.</strong>
				<a href={update.url} target="_blank" rel="noreferrer noopener">Read the notes</a>
				— Frog installs it on its own, usually within the hour.
			</p>
		{:else if update.state === 'current'}
			<p class="update faint">You are on the newest release.</p>
		{:else if update.state === 'failed'}
			<p class="update faint">Could not reach GitHub. Not to worry, nothing is wrong.</p>
		{/if}
	</section>
</div>

<style>
	.title {
		margin-bottom: 22px;
	}

	.block {
		margin-bottom: 44px;
	}

	.sub {
		font-size: 0.95rem;
		margin: 22px 0 10px;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(158px, 1fr));
		gap: 12px;
		margin-bottom: 14px;
	}

	.tiles.small {
		grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 14px;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		cursor: pointer;
		text-align: left;
		transition: border-color var(--fast) var(--ease);
	}

	.tile.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.tile .faint {
		font-size: 0.76rem;
	}

	.swatches {
		display: flex;
		gap: 4px;
		margin-bottom: 6px;
	}

	.swatches i {
		width: 20px;
		height: 20px;
		border-radius: 6px;
		border: 1px solid var(--line);
	}

	.mascots img {
		margin-bottom: 4px;
	}

	.rows {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
		cursor: pointer;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.78rem;
	}

	input[type='range'] {
		accent-color: var(--accent);
		min-width: 140px;
		flex: none;
	}

	input[type='checkbox'] {
		width: 20px;
		height: 20px;
		accent-color: var(--accent);
		flex: none;
	}

	.editor {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		padding: 14px 18px;
		margin-bottom: 14px;
	}

	summary {
		cursor: pointer;
		font-weight: 600;
		font-size: 0.92rem;
	}

	.colours {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
		gap: 12px;
		margin: 16px 0;
	}

	.colour {
		display: flex;
		align-items: center;
		gap: 9px;
		font-size: 0.84rem;
		cursor: pointer;
	}

	input[type='color'] {
		width: 34px;
		height: 34px;
		padding: 0;
		border: 1px solid var(--line-strong);
		border-radius: 8px;
		background: none;
		cursor: pointer;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	.input.name {
		width: 160px;
	}

	.btn.file {
		position: relative;
		overflow: hidden;
	}

	.btn.file input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.8rem;
	}

	.saved {
		list-style: none;
		padding: 0;
		margin: 16px 0 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.saved li {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 0.86rem;
	}

	.dots {
		display: inline-flex;
		gap: 3px;
	}

	.dots i {
		width: 13px;
		height: 13px;
		border-radius: 4px;
		border: 1px solid var(--line);
	}

	.saved strong {
		flex: 1;
		font-weight: 600;
	}

	.link {
		background: none;
		border: none;
		color: var(--text-dim);
		cursor: pointer;
		font-size: 0.8rem;
		text-decoration: underline;
		padding: 2px;
	}

	.link:hover {
		color: var(--text);
	}

	.link.danger:hover {
		color: var(--bad);
	}

	.builder {
		list-style: none;
		padding: 0;
		margin: 0 0 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.builder li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 11px 14px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.builder .label {
		flex: 1;
	}

	.nav-icon {
		display: grid;
		color: var(--text-faint);
	}

	.mini {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.76rem;
		color: var(--text-dim);
		cursor: pointer;
	}

	.mini input {
		width: 16px;
		height: 16px;
	}

	.link-row {
		color: inherit;
	}

	.link-row:hover {
		border-color: var(--line-strong);
	}

	.grip {
		color: var(--text-faint);
	}

	.icon-btn {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: 7px;
		border: 1px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		cursor: pointer;
	}

	.icon-btn:hover:not(:disabled) {
		color: var(--text);
	}

	.icon-btn:disabled {
		opacity: 0.35;
		cursor: not-allowed;
	}

	.code {
		font-family: var(--font-mono);
		font-size: 0.8rem;
		margin-top: 10px;
		resize: vertical;
	}

	.instance {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 18px;
		margin-bottom: 14px;
	}

	.facts {
		flex: 1;
		margin: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
		gap: 10px 20px;
	}

	.facts div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 0.86rem;
	}

	dt {
		color: var(--text-dim);
		font-size: 0.76rem;
	}

	dd {
		margin: 0;
		font-weight: 600;
	}

	dd.ok {
		color: var(--good);
	}
	dd.bad {
		color: var(--bad);
	}

	.tiny {
		font-size: 0.76rem;
		max-width: 70ch;
	}

	.update {
		margin-top: 12px;
		font-size: 0.86rem;
	}

	.update.available {
		padding: 11px 14px;
		border-radius: var(--radius-sm);
		background: var(--accent-soft);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
	}

	.update a {
		color: var(--accent);
		text-decoration: underline;
	}

	code {
		font-family: var(--font-mono);
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}

		.instance {
			flex-direction: column;
			align-items: flex-start;
		}
	}
</style>
