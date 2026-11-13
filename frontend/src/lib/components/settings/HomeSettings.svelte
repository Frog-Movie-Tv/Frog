<script lang="ts">
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import {
		GO_KEYS,
		HOME_LAYOUT,
		HOME_ROWS,
		MAX_TABS,
		NAV_PAGES,
		settings,
		type HeroStyle
	} from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	/** Home and menus: where Frog opens, the rows on Home, and the navigation. */
	const s = $derived(settings.current);

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

	const hiddenRows = $derived(HOME_ROWS.filter((r) => !s.homeRows.includes(r.id)));

	/** A layout kept from before has rows a new one leaves off. */
	const isLayout = $derived(
		s.homeRows.length === HOME_LAYOUT.length && s.homeRows.every((id, i) => id === HOME_LAYOUT[i])
	);

	function useLayout() {
		const before = [...s.homeRows];
		settings.set('homeRows', [...HOME_LAYOUT]);
		toasts.undoable('Home uses the new layout', () => settings.set('homeRows', before));
	}

	function rowInfo(id: string): { label: string; hint: string } | undefined {
		return HOME_ROWS.find((r) => r.id === id);
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

	/** The letter after G for each page, as set or as Frog has them. */
	const goKeys = $derived(s.goKeys ?? GO_KEYS);

	/** One letter a page; a letter taken by another page moves here. */
	function setGoKey(href: string, raw: string) {
		const letter = raw.trim().toLowerCase().slice(-1);
		const keys = { ...goKeys };
		for (const [page, key] of Object.entries(keys)) {
			if (key === letter) delete keys[page];
		}
		if (/^[a-z0-9]$/.test(letter) && letter !== 'g') keys[href] = letter;
		else delete keys[href];
		settings.set('goKeys', keys);
	}

	function toggleTab(href: string, on: boolean) {
		if (on && s.navTabs.length >= MAX_TABS) return;
		settings.set('navTabs', on ? [...s.navTabs, href] : s.navTabs.filter((h) => h !== href));
	}
</script>

<section class="block">
	<div class="section-head"><h2>Home</h2></div>

	<div class="rows list-pane">
		<div class="row">
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
					{ value: '/airing', label: 'Airing' },
					{ value: '/diary', label: 'Diary' },
					{ value: '/stats', label: 'Stats' }
				]}
				onchange={(v) => settings.set('landingPage', v)}
			/>
		</div>

		<div class="row">
			<span class="label">
				<strong>Hero</strong>
				<span class="faint">The title at the top of Home</span>
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
	</div>

	<h3 class="sub">Home rows</h3>
	<ul class="builder list-pane">
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

	{#if hiddenRows.length || !isLayout}
		<div class="actions">
			{#each hiddenRows as row (row.id)}
				<button class="btn ghost small" onclick={() => toggleRow(row.id)}>
					<Icon name="plus" size={14} />
					{row.label}
				</button>
			{/each}
			{#if !isLayout}
				<button class="btn small" onclick={useLayout}>Use the new layout</button>
			{/if}
		</div>
	{/if}
</section>

<section class="block">
	<div class="section-head">
		<h2>Navigation</h2>
		<span class="faint">{s.navTabs.length} of {MAX_TABS} in the phone tab bar</span>
	</div>

	<ul class="builder list-pane">
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
				<label class="mini key" title="Press G, then this letter, to go there">
					G
					<input
						class="input"
						value={goKeys[navPage.href] ?? ''}
						maxlength="1"
						aria-label="Shortcut letter for {navPage.label}"
						onchange={(e) => setGoKey(navPage.href, e.currentTarget.value)}
					/>
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

<style>
	/* Frog's own words here, never a person's, in rows with little room. */
	.block {
		margin-bottom: 44px;
		overflow-wrap: normal;
	}

	.sub {
		font-size: 0.95rem;
		margin: 22px 0 10px;
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

	input[type='checkbox']:not(.toggle-switch) {
		width: 20px;
		height: 20px;
		accent-color: var(--accent);
		flex: none;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
		align-items: center;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.8rem;
	}

	.builder {
		list-style: none;
		padding: 0;
		margin: 0 0 12px;
	}

	.builder li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 11px 14px;
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

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}
	}
	@media (max-width: 760px) {
		.icon-btn {
			width: 44px;
			height: 44px;
		}
	}

	/* Shortcuts are for a keyboard: a phone has no room for them, and no use. */
	@media (max-width: 760px), (hover: none) {
		.key {
			display: none;
		}
	}

	/* G, then a letter: the page's keyboard shortcut. */
	.key input {
		width: 2.2em;
		padding: 2px 4px;
		text-align: center;
		text-transform: lowercase;
		font-family: var(--font-mono);
	}
</style>
