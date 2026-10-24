<script lang="ts">
	import { goto } from '$app/navigation';
	import Logo from '#lib/components/Logo.svelte';
	import Rating from '#lib/components/Rating.svelte';
	import { SCORE_FORMATS, type ScoreFormat } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import {
		DEFAULTS,
		FONTS,
		HOME_ROWS,
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

	const hiddenRows = $derived(HOME_ROWS.filter((r) => !s.homeRows.includes(r.id)));

	let update = $state<{ state: 'idle' | 'checking' | 'current' | 'available' | 'failed'; tag?: string; url?: string }>({
		state: 'idle'
	});

	/**
	 * Deliberately a button rather than something automatic: Frog otherwise
	 * talks to nobody but TMDB, and a silent call home would undercut that.
	 */
	async function checkForUpdates() {
		update = { state: 'checking' };
		try {
			const res = await fetch('https://api.github.com/repos/MihCons/Frog/releases/latest', {
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
		<div class="section-head">
			<h2>Mascot</h2>
			<span class="faint">Changes the logo and the favicon</span>
		</div>

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
					<span class="faint">{m.hint}</span>
				</button>
			{/each}
		</div>
		<p class="faint tiny">
			An installed app keeps the icon it was installed with, so the home-screen icon changes when
			you reinstall it.
		</p>
	</section>

	<section class="block">
		<div class="section-head"><h2>Type and shape</h2></div>

		<div class="rows">
			<label class="row card">
				<span class="label">
					<strong>Font</strong>
					<span class="faint">Already on your device, so nothing is downloaded</span>
				</span>
				<select
					class="input auto"
					value={s.font}
					onchange={(e) => settings.set('font', e.currentTarget.value as FontName)}
				>
					{#each FONTS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
				</select>
			</label>

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
					<strong>Corner radius</strong><span class="faint">{Math.round(16 * s.radiusScale)}px</span>
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

			<label class="row card">
				<span class="label">
					<strong>Animation</strong><span class="faint">Page transitions and hover motion</span>
				</span>
				<select
					class="input auto"
					value={s.animations}
					onchange={(e) => settings.set('animations', e.currentTarget.value as AnimationLevel)}
				>
					<option value="full">Full</option>
					<option value="reduced">Reduced</option>
					<option value="off">Off</option>
				</select>
			</label>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Cards and grid</h2></div>

		<div class="rows">
			<label class="row card">
				<span class="label">
					<strong>Card style</strong><span class="faint">How the library draws each title</span>
				</span>
				<select
					class="input auto"
					value={s.cardStyle}
					onchange={(e) => settings.set('cardStyle', e.currentTarget.value as CardStyle)}
				>
					<option value="poster">Poster</option>
					<option value="landscape">Landscape</option>
					<option value="list">List</option>
					<option value="compact">Compact table</option>
				</select>
			</label>

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
				<span class="label"><strong>Grid spacing</strong><span class="faint">{s.gridGap}px</span></span>
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
			<label class="row card">
				<span class="label">
					<strong>Hero style</strong><span class="faint">The big image at the top of Home</span>
				</span>
				<select
					class="input auto"
					value={s.heroStyle}
					onchange={(e) => settings.set('heroStyle', e.currentTarget.value as HeroStyle)}
				>
					<option value="backdrop">Backdrop</option>
					<option value="poster">Poster</option>
					<option value="off">Off</option>
				</select>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Follow the poster</strong>
					<span class="faint">Tint each title page with a colour from its poster</span>
				</span>
				<input
					type="checkbox"
					checked={s.followPoster}
					onchange={(e) => settings.set('followPoster', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Open Frog on</strong><span class="faint">Where the app lands after sign-in</span>
				</span>
				<select
					class="input auto"
					value={s.landingPage}
					onchange={(e) => settings.set('landingPage', e.currentTarget.value)}
				>
					<option value="/">Home</option>
					<option value="/library">Library</option>
					<option value="/search">Search</option>
					<option value="/stats">Stats</option>
				</select>
			</label>
		</div>

		<h3 class="sub">Home rows</h3>
		<ul class="builder">
			{#each s.homeRows as id, i (id)}
				{@const row = HOME_ROWS.find((r) => r.id === id)}
				{#if row}
					<li>
						<span class="grip" aria-hidden="true">⠿</span>
						<span class="label">
							<strong>{row.label}</strong><span class="faint">{row.hint}</span>
						</span>
						<button class="icon-btn" disabled={i === 0} aria-label="Move up" onclick={() => moveRow(id, -1)}>↑</button>
						<button
							class="icon-btn"
							disabled={i === s.homeRows.length - 1}
							aria-label="Move down"
							onclick={() => moveRow(id, 1)}
						>
							↓
						</button>
						<button class="icon-btn" aria-label="Remove row" onclick={() => toggleRow(id)}>×</button>
					</li>
				{/if}
			{/each}
		</ul>

		{#if hiddenRows.length}
			<div class="actions">
				{#each hiddenRows as row (row.id)}
					<button class="btn ghost small" onclick={() => toggleRow(row.id)}>＋ {row.label}</button>
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
					<strong>Half stars</strong><span class="faint">Only affects the 5 star format</span>
				</span>
				<input
					type="checkbox"
					checked={s.halfStars}
					onchange={(e) => settings.set('halfStars', e.currentTarget.checked)}
				/>
			</label>

			<div class="row card">
				<span class="label">
					<strong>Live preview</strong>
					<span class="faint">
						Ratings are stored 0–100, so switching format never rounds anything
					</span>
				</span>
				<Rating value={preview} onchange={(v) => (preview = v)} />
			</div>
		</div>
	</section>

	<section class="block">
		<div class="section-head"><h2>Watching</h2></div>

		<div class="rows">
			<label class="row card">
				<span class="label">
					<strong>Default status</strong><span class="faint">Used by the ＋ button</span>
				</span>
				<select
					class="input auto"
					value={s.defaultStatus}
					onchange={(e) => settings.set('defaultStatus', e.currentTarget.value as Status)}
				>
					{#each STATUSES as st (st)}<option value={st}>{STATUS_LABEL[st]}</option>{/each}
				</select>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Spoiler protection</strong>
					<span class="faint">Blur stills for episodes you have not watched</span>
				</span>
				<input
					type="checkbox"
					checked={s.spoilerProtection}
					onchange={(e) => settings.set('spoilerProtection', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label">
					<strong>Hide animation in discovery</strong>
					<span class="faint">Keeps animated titles out of search results</span>
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
					<span class="faint">Fill started and finished dates as the status changes</span>
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
					<span class="faint">Mark a show completed when the last episode is ticked</span>
				</span>
				<input
					type="checkbox"
					checked={s.autoComplete}
					onchange={(e) => settings.set('autoComplete', e.currentTarget.checked)}
				/>
			</label>

			<label class="row card">
				<span class="label"><strong>Dates</strong></span>
				<select
					class="input auto"
					value={s.dateStyle}
					onchange={(e) => settings.set('dateStyle', e.currentTarget.value as 'friendly')}
				>
					<option value="friendly">Friendly (3 days ago)</option>
					<option value="long">Long (24 October 2026)</option>
					<option value="iso">ISO (2026-10-24)</option>
				</select>
			</label>

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
		<div class="section-head"><h2>Advanced</h2></div>

		<details class="editor">
			<summary>Custom CSS</summary>
			<p class="faint tiny">
				Applied to every page. Frog's own tokens are the easiest handle, for example
				<code>:root &#123; --accent: hotpink; &#125;</code>
			</p>
			<textarea
				class="input code"
				rows="7"
				spellcheck="false"
				placeholder=":root &#123; --radius: 4px; &#125;"
				value={s.customCss}
				onchange={(e) => settings.set('customCss', e.currentTarget.value)}
			></textarea>
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

	<section class="block">
		<div class="section-head"><h2>This instance</h2></div>

		<div class="instance card">
			<Logo size={38} />
			<dl class="facts">
				<div><dt>Signed in as</dt><dd>{session.status?.username ?? '—'}</dd></div>
				<div><dt>Version</dt><dd>{session.status?.version ?? '—'}</dd></div>
				<div><dt>Provider region</dt><dd>{session.status?.region ?? '—'}</dd></div>
				<div>
					<dt>TMDB key</dt>
					<dd class={session.status?.tmdb_configured ? 'ok' : 'bad'}>
						{session.status?.tmdb_configured ? 'configured' : 'missing'}
					</dd>
				</div>
			</dl>
		</div>

		<div class="actions">
			<a class="btn" href="/whats-new">What's new</a>
			<button class="btn ghost" onclick={checkForUpdates} disabled={update.state === 'checking'}>
				{update.state === 'checking' ? 'Checking…' : 'Check for updates'}
			</button>
			<button class="btn ghost" onclick={signOut}>Sign out</button>
		</div>

		{#if update.state === 'available'}
			<p class="update available">
				<strong>Version {update.tag} is out.</strong>
				<a href={update.url} target="_blank" rel="noreferrer noopener">Read the notes</a>
				— then run <code>git pull</code> and start Frog again.
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

	.input.auto {
		width: auto;
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

	.grip {
		color: var(--text-faint);
	}

	.icon-btn {
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
