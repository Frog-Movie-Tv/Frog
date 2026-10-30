<script lang="ts">
	import { titleHref } from '#lib/titles';
	import Icon from '#lib/components/Icon.svelte';
	import { api, img, type LibraryQuery } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Select from '#lib/components/Select.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { formatScore, plural, relativeDate, runtime, year } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	let items = $state<LibraryItem[]>([]);
	let loading = $state(true);
	let failed = $state(false);
	let sheetOpen = $state(false);
	let menuOpen = $state(false);

	let status = $state<Status | 'all'>('all');
	let kind = $state<Kind | 'all'>('all');
	let genre = $state('');
	let term = $state('');
	let favourite = $state(false);
	let sort = $state<NonNullable<LibraryQuery['sort']>>('added');
	let order = $state<'asc' | 'desc'>('desc');

	const filtered = $derived(
		status !== 'all' || kind !== 'all' || genre !== '' || term !== '' || favourite
	);

	/** How many filters are on, for the mobile button's badge. */
	const activeCount = $derived(
		[status !== 'all', kind !== 'all', genre !== '', term !== '', favourite].filter(Boolean).length
	);

	const genres = $derived(
		[...new Set(items.flatMap((i) => i.genres))].sort((a, b) => a.localeCompare(b))
	);

	const kindOptions = [
		{ value: 'all', label: 'Films & shows' },
		{ value: 'movie', label: 'Films' },
		{ value: 'tv', label: 'Shows' }
	];

	const genreOptions = $derived([
		{ value: '', label: 'Any genre' },
		...genres.map((g) => ({ value: g, label: g }))
	]);

	const sortOptions = [
		{ value: 'added', label: 'Recently added' },
		{ value: 'updated', label: 'Recently updated' },
		{ value: 'last_watched', label: 'Last watched' },
		{ value: 'name', label: 'Name' },
		{ value: 'rating', label: 'My rating' },
		{ value: 'release', label: 'Release date' }
	];

	const counts = $derived.by(() => {
		const out: Partial<Record<Status, number>> = {};
		for (const i of items) if (i.entry) out[i.entry.status] = (out[i.entry.status] ?? 0) + 1;
		return out;
	});

	$effect(() => {
		if (!session.authenticated) return;
		const query: LibraryQuery = {
			status: status === 'all' ? undefined : status,
			kind: kind === 'all' ? undefined : kind,
			genre: genre || undefined,
			q: term || undefined,
			favourite: favourite || undefined,
			sort,
			order
		};
		void load(query);
	});

	let token = 0;

	async function load(query: LibraryQuery) {
		const mine = ++token;
		loading = true;
		try {
			const all = await api.library(query);
			if (mine !== token) return;
			items = all.items;
			failed = false;
		} catch (e) {
			if (mine !== token) return;
			failed = true;
			toasts.error(e);
		} finally {
			if (mine === token) loading = false;
		}
	}

	function reset() {
		status = 'all';
		kind = 'all';
		genre = '';
		term = '';
		favourite = false;
	}

	const style = $derived(settings.current.cardStyle);
	const fmt = $derived(settings.current.scoreFormat);

	function progressOf(item: LibraryItem): number {
		return item.total_episodes > 0 ? (item.watched_episodes / item.total_episodes) * 100 : 0;
	}
</script>

<svelte:head>
	<title>Library · Frog</title>
</svelte:head>

<div class="page">
	<div class="section-head">
		<h1>Library</h1>
		<div class="head-tools">
			<span class="faint">{plural(items.length, 'title')}</span>
			<div class="menu-wrap">
				<button
					class="btn ghost icon-only"
					aria-label="Library options"
					aria-haspopup="menu"
					aria-expanded={menuOpen}
					onclick={() => (menuOpen = !menuOpen)}
				>
					<Icon name="more" size={18} />
				</button>
				{#if menuOpen}
					<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
					<div class="menu-catch" onclick={() => (menuOpen = false)}></div>
					<div class="menu" role="menu">
						<a role="menuitem" href="/import"><Icon name="upload" size={16} /> Import</a>
						<a role="menuitem" href="/api/export/library.json" download>
							<Icon name="download" size={16} /> Download backup
						</a>
						<a role="menuitem" href="/api/export/library.csv" download>
							<Icon name="download" size={16} /> Download spreadsheet
						</a>
					</div>
				{/if}
			</div>
		</div>
	</div>

	<!-- Desktop: everything laid out. Mobile: one button into a sheet. -->
	<div class="controls" class:open={sheetOpen}>
		<div class="chips" role="group" aria-label="Status filter">
			<button class="chip" class:on={status === 'all'} onclick={() => (status = 'all')}>All</button>
			{#each STATUSES as s (s)}
				<button class="chip" data-status={s} class:on={status === s} onclick={() => (status = s)}>
					<i class="sdot" aria-hidden="true"></i>
					{STATUS_LABEL[s]}
					{#if counts[s]}<span class="n">{counts[s]}</span>{/if}
				</button>
			{/each}
		</div>

		<div class="row">
			<input class="input search" placeholder="Filter by name" bind:value={term} />

			<Select
				label="Type"
				value={kind}
				options={kindOptions}
				onchange={(v) => (kind = v as Kind | 'all')}
			/>

			<Select label="Genre" value={genre} options={genreOptions} onchange={(v) => (genre = v)} />

			<Select
				label="Sort by"
				value={sort}
				options={sortOptions}
				onchange={(v) => (sort = v as NonNullable<LibraryQuery['sort']>)}
			/>

			<button
				class="btn ghost icon-only"
				aria-label="Reverse order"
				title="Reverse order"
				onclick={() => (order = order === 'desc' ? 'asc' : 'desc')}
			>
				<Icon name={order === 'desc' ? 'arrow-down' : 'arrow-up'} />
			</button>

			<button
				class="btn ghost icon-only"
				class:on={favourite}
				aria-pressed={favourite}
				aria-label="Favourites only"
				title="Favourites only"
				onclick={() => (favourite = !favourite)}
			>
				<Icon name="heart" filled={favourite} />
			</button>
		</div>
	</div>

	<div class="mobile-bar">
		<button class="btn" onclick={() => (sheetOpen = true)}>
			<Icon name="filter" size={16} />
			Filters
			{#if activeCount}<span class="count">{activeCount}</span>{/if}
		</button>
		{#if filtered}
			<button class="btn ghost" onclick={reset}>Clear</button>
		{/if}
	</div>

	{#if sheetOpen}
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="scrim" onclick={() => (sheetOpen = false)}></div>
		<div class="sheet" role="dialog" aria-modal="true" aria-label="Filters">
			<div class="handle" aria-hidden="true"></div>
			<div class="sheet-body">
				<div class="chips" role="group" aria-label="Status filter">
					<button class="chip" class:on={status === 'all'} onclick={() => (status = 'all')}>
						All
					</button>
					{#each STATUSES as s (s)}
						<button
							class="chip"
							data-status={s}
							class:on={status === s}
							onclick={() => (status = s)}
						>
							<i class="sdot" aria-hidden="true"></i>
							{STATUS_LABEL[s]}
							{#if counts[s]}<span class="n">{counts[s]}</span>{/if}
						</button>
					{/each}
				</div>

				<label class="field">
					<span>Name</span>
					<input class="input" placeholder="Filter by name" bind:value={term} />
				</label>

				<div class="field">
					<span>Type</span>
					<Select
						full
						label="Type"
						value={kind}
						options={kindOptions}
						onchange={(v) => (kind = v as Kind | 'all')}
					/>
				</div>

				<div class="field">
					<span>Genre</span>
					<Select
						full
						label="Genre"
						value={genre}
						options={genreOptions}
						onchange={(v) => (genre = v)}
					/>
				</div>

				<div class="field">
					<span>Sort</span>
					<Select
						full
						label="Sort"
						value={sort}
						options={sortOptions}
						onchange={(v) => (sort = v as NonNullable<LibraryQuery['sort']>)}
					/>
				</div>

				<div class="sheet-row">
					<button class="btn ghost" onclick={() => (order = order === 'desc' ? 'asc' : 'desc')}>
						<Icon name={order === 'desc' ? 'arrow-down' : 'arrow-up'} />
						{order === 'desc' ? 'Descending' : 'Ascending'}
					</button>
					<button class="btn ghost" class:on={favourite} onclick={() => (favourite = !favourite)}>
						<Icon name="heart" filled={favourite} />
						Favourites
					</button>
				</div>

				<div class="sheet-row">
					<button class="btn ghost" onclick={reset}>Clear all</button>
					<button class="btn primary grow" onclick={() => (sheetOpen = false)}>
						Show {plural(items.length, 'title')}
					</button>
				</div>
			</div>
		</div>
	{/if}

	{#if loading && !items.length}
		<Spinner label="Loading your library" />
	{:else if failed && !items.length}
		<Empty title="Could not load your library" hint="The server did not answer. Try again." />
	{:else if !items.length}
		<Empty
			art={filtered ? 'search' : 'rest'}
			title={filtered ? 'Nothing matches' : 'Your library is empty'}
			hint={filtered
				? 'Loosen a filter and it will come back.'
				: 'Everything you track will live here.'}
		>
			{#if filtered}
				<button class="btn" onclick={reset}>Clear filters</button>
			{:else}
				<a class="btn primary" href="/search">Find something to watch</a>
				<a class="btn ghost" href="/import">
					<Icon name="upload" size={16} />
					Import a library
				</a>
			{/if}
		</Empty>
	{:else if style === 'poster'}
		<Grid>
			{#each items as item (item.id)}
				<Poster
					morph
					kind={item.kind}
					ref={item}
					name={item.name}
					posterPath={item.poster_path}
					year={year(item.release_date)}
					status={item.entry?.status ?? null}
					rating={item.entry?.rating ?? null}
					watched={item.watched_episodes}
					total={item.total_episodes}
				/>
			{/each}
		</Grid>
	{:else if style === 'landscape'}
		<div class="landscape">
			{#each items as item (item.id)}
				<a class="land" href={titleHref(item)}>
					<div class="art">
						{#if item.backdrop_path || item.poster_path}
							<img
								src={img(item.backdrop_path ?? item.poster_path, 'w500')}
								alt=""
								loading="lazy"
							/>
						{/if}
						{#if item.total_episodes > 0 && settings.current.showProgress}
							<div class="bar"><i style:width="{progressOf(item)}%"></i></div>
						{/if}
					</div>
					<div class="land-meta">
						<strong>{item.name}</strong>
						<span class="faint">
							{year(item.release_date)}
							{#if item.entry?.status}· {STATUS_LABEL[item.entry.status]}{/if}
						</span>
					</div>
				</a>
			{/each}
		</div>
	{:else if style === 'list'}
		<ul class="list">
			{#each items as item (item.id)}
				<li>
					<a href={titleHref(item)}>
						{#if item.poster_path}
							<img src={img(item.poster_path, 'w92')} alt="" loading="lazy" />
						{/if}
						<span class="who">
							<strong>{item.name}</strong>
							<span class="faint">
								{year(item.release_date)} · {item.kind === 'tv' ? 'Show' : 'Film'}
								{#if item.genres.length}· {item.genres.slice(0, 2).join(', ')}{/if}
							</span>
						</span>
						{#if item.total_episodes > 0}
							<span class="faint nums">{item.watched_episodes}/{item.total_episodes}</span>
						{:else if item.runtime}
							<span class="faint nums">{runtime(item.runtime)}</span>
						{/if}
						<span class="score">{formatScore(item.entry?.rating ?? null, fmt)}</span>
					</a>
				</li>
			{/each}
		</ul>
	{:else}
		<div class="table-wrap">
			<table class="compact">
				<thead>
					<tr>
						<th>Title</th>
						<th>Year</th>
						<th>Type</th>
						<th>Status</th>
						<th>Progress</th>
						<th>Rating</th>
						<th>Last watched</th>
					</tr>
				</thead>
				<tbody>
					{#each items as item (item.id)}
						<tr>
							<td><a href={titleHref(item)}>{item.name}</a></td>
							<td class="num">{year(item.release_date)}</td>
							<td>{item.kind === 'tv' ? 'Show' : 'Film'}</td>
							<td>{item.entry ? STATUS_LABEL[item.entry.status] : '—'}</td>
							<td class="num">
								{item.total_episodes > 0 ? `${item.watched_episodes}/${item.total_episodes}` : '—'}
							</td>
							<td class="num">{formatScore(item.entry?.rating ?? null, fmt)}</td>
							<td>{relativeDate(item.last_watched_at) || '—'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.controls {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 26px;
	}

	.sdot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--status);
		flex: none;
	}

	.chip.on .sdot {
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-ink) 70%, transparent);
	}

	.head-tools {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.menu-wrap {
		position: relative;
	}

	.menu-catch {
		position: fixed;
		inset: 0;
		z-index: 60;
	}

	.menu {
		position: absolute;
		z-index: 61;
		right: 0;
		top: calc(100% + 6px);
		min-width: 220px;
		padding: 6px;
		border-radius: var(--radius);
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
	}

	.menu a {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 11px;
		border-radius: var(--radius-sm);
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.menu a:hover {
		background: var(--bg-hover);
		color: var(--text);
	}

	.chip .n {
		font-size: 0.74rem;
		opacity: 0.75;
		font-variant-numeric: tabular-nums;
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		align-items: center;
	}

	.row .input {
		width: auto;
		padding: 9px 12px;
		font-size: 0.86rem;
	}

	.search {
		flex: 1;
		min-width: 190px;
	}

	.btn.on {
		color: var(--bad);
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
	}

	.icon-only {
		padding: 9px 11px;
	}

	.mobile-bar {
		display: none;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 20px;
	}

	.count {
		display: inline-grid;
		place-items: center;
		min-width: 19px;
		height: 19px;
		margin-left: 6px;
		padding: 0 5px;
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.7rem;
	}

	.scrim {
		position: fixed;
		inset: 0;
		z-index: 70;
		background: rgba(0, 0, 0, 0.5);
	}

	.sheet {
		position: fixed;
		z-index: 71;
		inset: auto 0 0 0;
		max-height: 86dvh;
		overflow-y: auto;
		background: var(--bg-elev);
		border-top: 1px solid var(--line-strong);
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		box-shadow: var(--shadow-3);
		padding-bottom: calc(14px + env(safe-area-inset-bottom));
	}

	.handle {
		width: 42px;
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		margin: 10px auto 4px;
	}

	.sheet-body {
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 10px 18px 6px;
	}

	.sheet-row {
		display: flex;
		gap: 10px;
	}

	.grow {
		flex: 1;
	}

	.landscape {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: var(--gap);
	}

	.land .art {
		position: relative;
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.land img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.bar {
		position: absolute;
		inset: auto 0 0 0;
		height: 3px;
		background: rgba(255, 255, 255, 0.2);
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.land-meta {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: 8px;
		font-size: 0.86rem;
	}

	.land-meta .faint {
		font-size: 0.74rem;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.list a {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.list a:hover {
		border-color: var(--line-strong);
	}

	.list img {
		width: 38px;
		border-radius: 5px;
		display: block;
	}

	.who {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.who strong {
		font-size: 0.92rem;
	}

	.who .faint {
		font-size: 0.76rem;
	}

	.nums {
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}

	.score {
		min-width: 4ch;
		text-align: right;
		font-weight: 650;
		font-size: 0.88rem;
	}

	.table-wrap {
		overflow-x: auto;
		border: 1px solid var(--line);
		border-radius: var(--radius);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.86rem;
	}

	th,
	td {
		text-align: left;
		padding: 10px 14px;
		border-bottom: 1px solid var(--line);
		white-space: nowrap;
	}

	th {
		font-size: 0.74rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-faint);
		background: var(--bg-elev);
		position: sticky;
		top: 0;
	}

	tbody tr:hover {
		background: var(--bg-elev);
	}

	tbody tr:last-child td {
		border-bottom: none;
	}

	.num {
		font-variant-numeric: tabular-nums;
	}

	@media (max-width: 760px) {
		.controls {
			display: none;
		}

		.controls.open {
			display: none;
		}

		.mobile-bar {
			display: flex;
		}
	}
</style>
