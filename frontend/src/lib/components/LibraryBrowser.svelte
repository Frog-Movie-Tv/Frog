<script module lang="ts">
	import type { LibraryAnswer } from '#lib/api';
	import type { SharedLibrary } from '#lib/types';

	/** One reading: all of my own library, or pages of someone else's. */
	type Answer = LibraryAnswer | SharedLibrary;

	/**
	 * The last answer, so Back from a title draws the same grid at once and the
	 * browser has something to put the scroll position on. A fresh reading
	 * replaces it straight after.
	 */
	let last: { key: string; answer: Answer } | null = null;

	/** How many of someone else's titles to read at a time. */
	const PAGE = 60;
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import { afterNavigate, goto } from '$app/navigation';
	import { address } from '#lib/address';
	import { titleHref } from '#lib/titles';
	import { SvelteSet } from 'svelte/reactivity';
	import Icon from '#lib/components/Icon.svelte';
	import { api, ApiError, img, type LibraryQuery } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import LibraryEditBar from '#lib/components/LibraryEditBar.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Select from '#lib/components/Select.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { formatScore, plural, relativeDate, runtime, year } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { uniqueBy } from '#lib/unique';
	import type { Kind, SharedItem, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	interface Props {
		/** Whose library, read only. Left out, it is your own. */
		owner?: string;
	}

	let { owner }: Props = $props();

	type Sort = NonNullable<LibraryQuery['sort']>;
	type Order = 'asc' | 'desc';
	type Query = Pick<URLSearchParams, 'get'>;

	const SORTS: Sort[] = ['added', 'updated', 'last_watched', 'name', 'rating', 'release'];
	/** Names read from A; everything else from the newest or the highest. */
	const naturalOrder = (s: Sort): Order => (s === 'name' ? 'asc' : 'desc');

	// The filters live in the address, as Search's do, so Back from a title
	// lands on the same list and a filtered library can be shared as a link.
	const start = address().searchParams;
	const startSort = SORTS.find((s) => s === start.get('sort')) ?? 'added';

	let answer = $state<Answer | null>(null);
	let loading = $state(true);
	let failed = $state(false);
	/** Someone else's library that is private, or was never there. */
	let missing = $state(false);
	let sheetOpen = $state(false);
	let menuOpen = $state(false);

	let status = $state<Status | 'all'>(STATUSES.find((s) => s === start.get('status')) ?? 'all');
	let kind = $state<Kind | 'all'>(kindFrom(start));
	let genre = $state(start.get('genre') ?? '');
	let term = $state(start.get('q') ?? '');
	let favourite = $state(start.has('favourite'));
	let sort = $state<Sort>(startSort);
	let order = $state<Order>(orderFrom(start, startSort));

	function kindFrom(params: Query): Kind | 'all' {
		const k = params.get('kind');
		return k === 'tv' || k === 'movie' ? k : 'all';
	}

	function orderFrom(params: Query, s: Sort): Order {
		const o = params.get('order');
		return o === 'asc' || o === 'desc' ? o : naturalOrder(s);
	}

	const name = $derived(term.trim());
	const items = $derived<SharedItem[]>(answer?.items ?? []);
	/** How many titles the filters show, loaded or not. */
	const total = $derived(answer && 'total' in answer ? answer.total : items.length);
	const shared = $derived(answer && 'owner' in answer ? answer.owner : null);
	const ownerName = $derived(shared?.display_name || shared?.username || owner || '');
	const home = $derived(owner ? `/people/${encodeURIComponent(owner)}/library` : '/library');
	const counts = $derived(answer?.counts ?? {});
	const everyStatus = $derived(Object.values(counts).reduce((n, c) => n + (c ?? 0), 0));

	const filtered = $derived(
		status !== 'all' || kind !== 'all' || genre !== '' || name !== '' || favourite
	);

	/** How many filters are on, for the mobile button's badge. */
	const activeCount = $derived(
		[status !== 'all', kind !== 'all', genre !== '', name !== '', favourite].filter(Boolean).length
	);

	const kindOptions = [
		{ value: 'all', label: 'Films & shows' },
		{ value: 'movie', label: 'Films' },
		{ value: 'tv', label: 'Shows' }
	];

	const genreOptions = $derived([
		{ value: '', label: 'Any genre' },
		// A link can name a genre this library no longer has; it still reads true.
		...[...new Set([...(answer?.genres ?? []), ...(genre ? [genre] : [])])].map((g) => ({
			value: g,
			label: g
		}))
	]);

	const sortOptions = $derived([
		{ value: 'added', label: 'Recently added' },
		{ value: 'updated', label: 'Recently updated' },
		{ value: 'last_watched', label: 'Last watched' },
		{ value: 'name', label: 'Name' },
		{ value: 'rating', label: owner ? 'Rating' : 'My rating' },
		{ value: 'release', label: 'Release date' }
	]);

	/** The address these filters have, leaving out whatever is at its default. */
	function search(): string {
		const pairs: [string, string][] = [];
		if (status !== 'all') pairs.push(['status', status]);
		if (kind !== 'all') pairs.push(['kind', kind]);
		if (genre) pairs.push(['genre', genre]);
		if (name) pairs.push(['q', name]);
		if (favourite) pairs.push(['favourite', '1']);
		if (sort !== 'added') pairs.push(['sort', sort]);
		if (order !== naturalOrder(sort)) pairs.push(['order', order]);
		const text = new URLSearchParams(pairs).toString();
		return text ? `?${text}` : '';
	}

	function currentQuery(): LibraryQuery {
		return {
			status: status === 'all' ? undefined : status,
			kind: kind === 'all' ? undefined : kind,
			genre: genre || undefined,
			q: name || undefined,
			favourite: favourite || undefined,
			sort,
			order
		};
	}

	/** Who is asking as well as what: a shared device has more than one library. */
	const keyOf = (text: string) => `${session.status?.username ?? ''}${owner ?? ''}${text}`;

	const firstKey = keyOf(search());
	if (last?.key === firstKey) {
		answer = last.answer;
		loading = false;
	}

	/** A typed name waits for a pause; the rest of the filters apply at once. */
	let sent = untrack(() => name);

	$effect(() => {
		if (!session.authenticated) return;
		const query = currentQuery();
		const wanted = search();
		const timer = setTimeout(
			() => {
				sent = name;
				if (wanted !== untrack(() => address().search)) {
					void goto(`${home}${wanted}`, { shallow: true, replace: true });
				}
				void load(query, keyOf(wanted));
			},
			name === sent ? 0 : 220
		);
		return () => clearTimeout(timer);
	});

	// Choosing Library in the menu while filtered is a new address, not ours.
	afterNavigate((navigation) => {
		if (navigation.shallow || navigation.type === 'enter') return;
		const params = address().searchParams;
		if (params.toString() === search().slice(1)) return;
		status = STATUSES.find((s) => s === params.get('status')) ?? 'all';
		kind = kindFrom(params);
		genre = params.get('genre') ?? '';
		term = params.get('q') ?? '';
		favourite = params.has('favourite');
		sort = SORTS.find((s) => s === params.get('sort')) ?? 'added';
		order = orderFrom(params, sort);
	});

	let token = 0;

	async function load(query: LibraryQuery, key: string) {
		const mine = ++token;
		loading = true;
		try {
			// Everything already in view, so coming back does not shrink the page.
			const read = owner
				? await api.profileLibrary(owner, { ...query, limit: Math.max(PAGE, items.length) })
				: await api.library(query);
			if (mine !== token) return;
			answer = read;
			last = { key, answer: read };
			failed = false;
			missing = false;
		} catch (e) {
			if (mine !== token) return;
			failed = true;
			missing = !!owner && e instanceof ApiError && e.status === 404;
			if (!missing) toasts.error(e);
		} finally {
			if (mine === token) loading = false;
		}
	}

	/** The next page of someone else's library. */
	async function loadMore() {
		if (!owner || loading || !answer || items.length >= total) return;
		const mine = ++token;
		const before = answer;
		loading = true;
		try {
			const read = await api.profileLibrary(owner, {
				...currentQuery(),
				limit: PAGE,
				offset: items.length
			});
			if (mine !== token) return;
			answer = {
				...read,
				items: uniqueBy([...before.items, ...read.items], (i) => i.id)
			};
			last = { key: keyOf(search()), answer };
		} catch (e) {
			if (mine === token) toasts.error(e);
		} finally {
			if (mine === token) loading = false;
		}
	}

	/** Reads on as the end of the list comes near. */
	function reachedEnd(node: HTMLElement) {
		const seen = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting)) void loadMore();
			},
			{ rootMargin: '600px' }
		);
		seen.observe(node);
		return { destroy: () => seen.disconnect() };
	}

	function reset() {
		status = 'all';
		kind = 'all';
		genre = '';
		term = '';
		favourite = false;
	}

	function chooseSort(value: string) {
		sort = value as Sort;
		order = naturalOrder(sort);
	}

	const style = $derived(settings.current.cardStyle);
	const fmt = $derived(settings.current.scoreFormat);

	function progressOf(item: SharedItem): number {
		return item.total_episodes > 0 ? (item.watched_episodes / item.total_episodes) * 100 : 0;
	}

	// Edit mode: a click ticks a title instead of opening it, in every view.
	let editing = $state(false);
	const picked = new SvelteSet<number>();
	/** What is ticked and still on screen: a filter can take a title away. */
	const chosen = $derived(items.filter((i) => picked.has(i.id)));

	function toggle(id: number) {
		if (picked.has(id)) picked.delete(id);
		else picked.add(id);
	}

	function selectAll() {
		const every = chosen.length === items.length;
		picked.clear();
		if (!every) for (const item of items) picked.add(item.id);
	}

	function stopEditing() {
		editing = false;
		picked.clear();
	}

	/** What makes a title's link a tick box while editing. */
	function pickable(id: number) {
		if (!editing) return {};
		return {
			role: 'checkbox',
			'aria-checked': picked.has(id),
			onclick: (e: MouseEvent) => {
				e.preventDefault();
				e.stopPropagation();
				toggle(id);
			},
			onkeydown: (e: KeyboardEvent) => {
				if (e.key !== ' ') return;
				e.preventDefault();
				toggle(id);
			}
		};
	}

	/** After an action: the list as it is now, and nothing ticked. */
	async function changed() {
		picked.clear();
		await load(currentQuery(), keyOf(search()));
	}
</script>

{#snippet tick(id: number)}
	{#if editing}
		<span class="tick" class:on={picked.has(id)} aria-hidden="true">
			{#if picked.has(id)}<Icon name="check" size={14} />{/if}
		</span>
	{/if}
{/snippet}

<svelte:window
	onkeydown={(e) => {
		if (e.key !== 'Escape') return;
		menuOpen = false;
		sheetOpen = false;
	}}
/>

<div class="page">
	{#if owner}
		<a class="back faint hit" href="/people/{encodeURIComponent(owner)}">
			<Icon name="chevron-left" size={16} />
			{ownerName}
		</a>
	{/if}
	<div class="page-head">
		<h1>Library</h1>
		<div class="head-tools actions">
			{#if answer}<span class="faint">{plural(total, 'title')}</span>{/if}
			{#if !owner && items.length && !editing}
				<button class="btn ghost" onclick={() => (editing = true)}>
					<Icon name="pencil" size={15} /> Edit
				</button>
			{/if}
			<div class="menu-wrap" class:gone={owner}>
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
			<button class="chip" class:on={status === 'all'} onclick={() => (status = 'all')}>
				All
				{#if everyStatus}<span class="n">{everyStatus}</span>{/if}
			</button>
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

			<Select label="Sort by" value={sort} options={sortOptions} onchange={chooseSort} />

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

			{#if filtered}
				<button class="btn ghost" onclick={reset}>Clear</button>
			{/if}
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
						{#if everyStatus}<span class="n">{everyStatus}</span>{/if}
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
					<Select full label="Sort" value={sort} options={sortOptions} onchange={chooseSort} />
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
						Show {plural(total, 'title')}
					</button>
				</div>
			</div>
		</div>
	{/if}

	{#if missing}
		<Empty
			art="search"
			title="No profile here"
			hint="It may be private, or there is no such account on this Frog."
		>
			<a class="btn" href="/people">Everyone sharing</a>
		</Empty>
	{:else if loading && !items.length}
		<Spinner label={owner ? 'Loading this library' : 'Loading your library'} />
	{:else if failed && !items.length}
		<Empty
			title={owner ? 'Could not load this library' : 'Could not load your library'}
			hint="The server did not answer. Try again."
		/>
	{:else if !items.length}
		<Empty
			art={filtered ? 'search' : 'rest'}
			title={filtered ? 'Nothing matches' : owner ? 'Nothing here yet' : 'Your library is empty'}
			hint={filtered
				? 'Loosen a filter and it will come back.'
				: owner
					? undefined
					: 'Everything you track will live here.'}
		>
			{#if filtered}
				<button class="btn" onclick={reset}>Clear filters</button>
			{:else if !owner}
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
					{...pickable(item.id)}
					morph={!editing}
					selected={editing ? picked.has(item.id) : undefined}
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
				<a
					class="land"
					class:on={picked.has(item.id)}
					href={titleHref(item)}
					{...pickable(item.id)}
				>
					<div class="art">
						{@render tick(item.id)}
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
					<a class:on={picked.has(item.id)} href={titleHref(item)} {...pickable(item.id)}>
						{@render tick(item.id)}
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
						{#if editing}<th aria-label="Selected"></th>{/if}
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
						<tr
							class:on={picked.has(item.id)}
							onclick={editing ? () => toggle(item.id) : undefined}
						>
							{#if editing}<td>{@render tick(item.id)}</td>{/if}
							<td><a href={titleHref(item)} {...pickable(item.id)}>{item.name}</a></td>
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

	{#if items.length < total}
		<div class="more" use:reachedEnd>
			<button class="btn ghost" disabled={loading} onclick={loadMore}>Show more</button>
		</div>
	{/if}

	{#if editing}
		<div class="bar-room" aria-hidden="true"></div>
		<LibraryEditBar
			{chosen}
			shown={items.length}
			onselectall={selectAll}
			ondone={stopEditing}
			onchanged={changed}
		/>
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

	.sheet .chips {
		flex-wrap: wrap;
		overflow: visible;
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

	/* Edit mode: a round tick where a title would otherwise just be a link. */
	.tick {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		border: 2px solid var(--line-strong);
		background: var(--bg-elev);
		color: var(--accent-ink);
	}

	.tick.on {
		border-color: var(--accent);
		background: var(--accent);
	}

	.land .tick {
		position: absolute;
		z-index: 1;
		top: 7px;
		left: 7px;
	}

	.land.on .art,
	.list a.on {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	tr.on td {
		background: var(--accent-soft);
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-bottom: 8px;
		font-size: 0.86rem;
		text-decoration: none;
	}

	.gone {
		display: none;
	}

	.more {
		display: grid;
		place-items: center;
		padding: 24px 0;
	}

	/* The last row clears the bar. */
	.bar-room {
		height: 160px;
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
