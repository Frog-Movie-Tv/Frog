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

	/** Shortcuts are set rarely, so their fields wait behind one button. */
	let editingKeys = $state(false);

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
					<span class="label">
						<strong>{row.label}</strong><span class="faint">{row.hint}</span>
					</span>
					<span class="order">
						<button
							class="icon-btn"
							disabled={i === 0}
							aria-label="Move {row.label} up"
							onclick={() => moveRow(id, -1)}
						>
							<Icon name="chevron-up" size={15} />
						</button>
						<button
							class="icon-btn"
							disabled={i === s.homeRows.length - 1}
							aria-label="Move {row.label} down"
							onclick={() => moveRow(id, 1)}
						>
							<Icon name="chevron-down" size={15} />
						</button>
					</span>
					<button class="icon-btn" aria-label="Remove {row.label}" onclick={() => toggleRow(id)}>
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
		<button class="btn ghost small keys-toggle" onclick={() => (editingKeys = !editingKeys)}>
			{editingKeys ? 'Done' : 'Edit shortcuts'}
		</button>
	</div>

	<ul class="builder nav list-pane">
		<li class="legend" aria-hidden="true">
			<span class="label"></span>
			<span class="col">Tab bar <span class="count">{s.navTabs.length}/{MAX_TABS}</span></span>
			<span class="col">Menu</span>
			<span class="order"></span>
		</li>
		{#each navPages as navPage, i (navPage.href)}
			{@const inBar = !s.navHidden.includes(navPage.href)}
			{@const inTabs = s.navTabs.includes(navPage.href)}
			{@const key = goKeys[navPage.href]}
			<li>
				<span class="nav-icon"><Icon name={navPage.icon as IconName} size={17} /></span>
				<span class="label">
					<strong>{navPage.label}</strong>
					{#if editingKeys}
						<label class="key">
							G then
							<input
								class="input"
								value={key ?? ''}
								maxlength="1"
								aria-label="Shortcut letter for {navPage.label}"
								onchange={(e) => setGoKey(navPage.href, e.currentTarget.value)}
							/>
						</label>
					{:else if key}
						<span class="key faint" title="Press G, then {key.toUpperCase()}"
							><kbd>G</kbd><kbd>{key.toUpperCase()}</kbd></span
						>
					{/if}
				</span>
				<span class="col">
					<button
						class="tab-toggle"
						class:on={inTabs}
						aria-pressed={inTabs}
						aria-label="{navPage.label} in the phone tab bar"
						title={!inTabs && s.navTabs.length >= MAX_TABS
							? `The tab bar holds ${MAX_TABS}`
							: 'In the phone tab bar'}
						disabled={!inTabs && s.navTabs.length >= MAX_TABS}
						onclick={() => toggleTab(navPage.href, !inTabs)}
					>
						<Icon name="phone" size={16} filled={inTabs} />
					</button>
				</span>
				<span class="col">
					<input
						type="checkbox"
						class="toggle-switch"
						aria-label="{navPage.label} in the menu"
						checked={inBar || navPage.fixed}
						disabled={navPage.fixed}
						onchange={(e) => toggleHidden(navPage.href, !e.currentTarget.checked)}
					/>
				</span>
				<span class="order">
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
				</span>
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
		min-height: 54px;
	}

	.builder .label {
		flex: 1;
	}

	/* One label over each column, rather than the same word on every row. */
	.nav .legend {
		min-height: 0;
		padding-top: 9px;
		padding-bottom: 7px;
		font-size: 0.72rem;
		font-weight: 650;
		color: var(--text-faint);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.nav .legend .col {
		white-space: nowrap;
	}

	.nav .legend .count {
		margin-left: 4px;
		font-weight: 500;
		text-transform: none;
	}

	/* A page's name with its shortcut beside it, so every row is one height. */
	.nav .label {
		flex-direction: row;
		align-items: center;
		gap: 10px;
	}

	.col {
		flex: none;
		width: 76px;
		display: flex;
		justify-content: center;
	}

	.nav-icon {
		display: grid;
		color: var(--text-faint);
	}

	.key {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.74rem;
		color: var(--text-dim);
	}

	kbd {
		min-width: 1.5em;
		padding: 0 4px;
		border: 1px solid var(--line);
		border-radius: 5px;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		text-align: center;
	}

	.key input {
		width: 2.2em;
		padding: 2px 4px;
		text-align: center;
		text-transform: lowercase;
		font-family: var(--font-mono);
	}

	.tab-toggle {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border-radius: 10px;
		border: 1px solid var(--line);
		background: transparent;
		color: var(--text-faint);
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.tab-toggle.on {
		background: var(--accent-soft);
		border-color: transparent;
		color: var(--accent);
	}

	.tab-toggle:disabled {
		opacity: 0.35;
		cursor: not-allowed;
	}

	.order {
		flex: none;
		display: flex;
		gap: 4px;
		width: 60px;
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

	/* With a mouse, moving a row is offered where the pointer is, so the
	   list reads as names and switches until then. */
	@media (hover: hover) {
		.builder li .order {
			opacity: 0;
			transition: opacity var(--fast) var(--ease);
		}

		.builder li:hover .order,
		.builder li:focus-within .order {
			opacity: 1;
		}
	}

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}

		.col {
			width: 52px;
		}

		.nav .legend {
			font-size: 0.66rem;
		}
	}

	@media (max-width: 760px) {
		.order {
			width: auto;
		}

		/* Big enough for a finger. */
		.icon-btn,
		.tab-toggle {
			width: 44px;
			height: 44px;
		}
	}

	/* Shortcuts are for a keyboard: a phone has no room for them, and no use. */
	@media (max-width: 760px), (hover: none) {
		.key,
		.keys-toggle {
			display: none;
		}
	}
</style>
