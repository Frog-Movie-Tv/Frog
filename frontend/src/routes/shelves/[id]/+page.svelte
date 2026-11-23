<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { address } from '#lib/address';
	import { api, frogId, img, SEARCH_PAUSE } from '#lib/api';
	import { lastShelf, readShelf } from '#lib/bookcase';
	import { sortItems, type ItemSort } from '#lib/books';
	import BookRows from '#lib/components/BookRows.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import WatchlistTally from '#lib/components/WatchlistTally.svelte';
	import { settings } from '#lib/settings.svelte';
	import ShelfFromList from '#lib/components/ShelfFromList.svelte';
	import ShelfRules from '#lib/components/ShelfRules.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { year, plural } from '#lib/format';
	import { titleMenu } from '#lib/menus';
	import { morph } from '#lib/morph.svelte';
	import { session } from '#lib/session.svelte';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { Length, LibraryItem, SearchItem, Shelf, ShelfRules as Rules } from '#lib/types';

	const id = $derived(Number(page.params.id));

	// Opened from the bookcase, the shelf was read as the pointer came near:
	// it draws at once, so the zoom has its books to land on.
	const kept = lastShelf(Number(page.params.id));
	let shelf = $state<Shelf | null>(kept?.shelf ?? null);
	let items = $state<LibraryItem[]>(kept?.items ?? []);
	let lengths = $state<Record<string, Length>>(kept?.lengths ?? {});
	let loading = $state(!kept);
	let error = $state<Failure | null>(null);
	let editing = $state(false);
	/** Adding a pasted list, in its order. */
	let listing = $state(false);
	let editingRules = $state(page.url.searchParams.get('edit') === '1');
	let renaming = $state(false);
	let draftName = $state('');

	async function load() {
		try {
			const r = await readShelf(id);
			shelf = r.shelf;
			items = r.items;
			lengths = r.lengths;
			error = null;
			// A shelf's card can send you here to rename it, or to add to it.
			const asked = page.url.searchParams;
			if (asked.get('rename') === '1' && !renaming) startRename();
			if (asked.get('add') === '1' && !adding && shelf.kind !== 'smart') void openAdding();
		} catch (e) {
			error = failure(e, {
				title: 'No shelf here',
				hint: 'It may have been deleted, or it belongs to someone else.'
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (!session.authenticated || !id) return;
		void load();
	});

	/** The order to show it in lives in the address, so Back from a title keeps it. */
	const SORTS: { value: ItemSort; label: string }[] = [
		{ value: 'order', label: 'Your order' },
		{ value: 'year', label: 'Year' }
	];
	const sorts = $derived(
		shelf?.kind === 'watchlist'
			? [
					...SORTS,
					{ value: 'short' as const, label: 'Shortest first' },
					{ value: 'long' as const, label: 'Longest first' }
				]
			: [...SORTS, { value: 'name' as const, label: 'Name' }]
	);
	const wanted = $derived(address().searchParams);
	const sortBy = $derived(sorts.find((s) => s.value === wanted.get('sort'))?.value ?? 'order');
	const oldest = $derived(wanted.get('oldest') === '1');
	const shown = $derived(sortItems(items, sortBy, lengths, oldest));

	function sortTo(by: ItemSort) {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const params = new URLSearchParams(untrack(() => address().search));
		// Year again turns it round: newest first, then oldest.
		const flip = by === 'year' && sortBy === 'year';
		if (by === 'order') params.delete('sort');
		else params.set('sort', by);
		if (flip && !oldest) params.set('oldest', '1');
		else params.delete('oldest');
		const search = params.size ? `?${params}` : '';
		void goto(`/shelves/${id}${search}`, { shallow: true, replace: true, state: page.state });
	}

	let phone = $state(false);
	$effect(() => {
		const query = matchMedia('(max-width: 720px)');
		phone = query.matches;
		const changed = () => (phone = query.matches);
		query.addEventListener('change', changed);
		return () => query.removeEventListener('change', changed);
	});
	/** Three-quarters on a phone, as the size in Settings does. */
	const size = $derived(Math.round(settings.current.posterSize * (phone ? 0.79 : 1)));

	async function saveOrder() {
		try {
			await api.reorderShelf(
				id,
				items.map((i) => i.id)
			);
		} catch (e) {
			toasts.error(e);
		}
	}

	async function remove(item: LibraryItem) {
		const before = items;
		items = items.filter((i) => i.id !== item.id);
		try {
			await api.removeFromShelf(id, item.id);
			if (shelf) shelf.count = items.length;
			toasts.undoable(`Removed ${item.name}`, async () => {
				await api.addToShelf(id, item.id);
				await api.reorderShelf(
					id,
					before.map((i) => i.id)
				);
				await load();
			});
		} catch (e) {
			items = before;
			toasts.error(e);
		}
	}

	function startRename() {
		draftName = shelf?.name ?? '';
		renaming = true;
	}

	/** Ready to type over: the old name selected, as in any rename. */
	function focusName(input: HTMLInputElement) {
		input.focus();
		input.select();
	}

	/** Adding from your own library, without a trip to each title's page. */
	let adding = $state(false);
	let library = $state<LibraryItem[]>([]);
	let pick = $state('');
	const candidates = $derived.by(() => {
		const needle = pick.trim().toLowerCase();
		const on = new Set(items.map((i) => i.id));
		return library
			.filter((i) => !on.has(i.id) && (!needle || i.name.toLowerCase().includes(needle)))
			.slice(0, 40);
	});

	/** A watchlist picks from everything, so it searches as you type. */
	const watchlist = $derived(shelf?.kind === 'watchlist');
	let found = $state<SearchItem[]>([]);
	let searching = $state(false);
	let asked = 0;

	$effect(() => {
		if (!adding || !watchlist) return;
		const q = pick.trim();
		const ask = ++asked;
		if (q.length < 2) {
			found = [];
			searching = false;
			return;
		}
		searching = true;
		const calling = new AbortController();
		const wait = setTimeout(() => {
			api
				.search(q, 1, {}, calling.signal)
				.then((r) => {
					if (ask === asked) found = r.results.slice(0, 24);
				})
				.catch(() => undefined)
				.finally(() => {
					if (ask === asked) searching = false;
				});
		}, SEARCH_PAUSE);
		return () => {
			clearTimeout(wait);
			calling.abort();
		};
	});

	const onShelf = $derived(new Set(items.map((i) => i.id)));

	async function addFound(result: SearchItem) {
		try {
			const titleId = await frogId(result);
			if (onShelf.has(titleId)) return;
			await api.addToShelf(id, titleId);
			toasts.push(`Added ${result.name}`);
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	async function openAdding() {
		adding = true;
		if (watchlist || library.length) return;
		try {
			library = (await api.library({ sort: 'name', order: 'asc', limit: 2000 })).items;
		} catch (e) {
			toasts.error(e);
		}
	}

	async function add(item: LibraryItem) {
		try {
			await api.addToShelf(id, item.id);
			items = [...items, item];
			if (shelf) shelf.count = items.length;
		} catch (e) {
			toasts.error(e);
		}
	}

	async function rename(event: SubmitEvent) {
		event.preventDefault();
		if (!shelf || !draftName.trim()) return;
		try {
			const r = await api.updateShelf(id, { name: draftName.trim() });
			shelf = r.shelf;
			renaming = false;
		} catch (e) {
			toasts.error(e);
		}
	}

	async function saveRules(rules: Rules) {
		try {
			await api.updateShelf(id, { rules });
			editingRules = false;
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	async function destroy() {
		if (!shelf) return;
		const name = shelf.name;
		try {
			const gone = await api.deleteShelf(id);
			await goto('/shelves');
			toasts.undoable(`Deleted ${name}`, async () => {
				const { shelf: back } = await api.createShelf(gone);
				await goto(`/shelves/${back.id}`);
			});
		} catch (e) {
			toasts.error(e);
		}
	}
</script>

<svelte:head>
	<title
		>{shelf ? `${shelf.name} · Frog` : error ? `${error.heading} · Frog` : 'Shelf · Frog'}</title
	>
</svelte:head>

<div class="page" style:--glow={shelf?.accent ?? undefined}>
	<a class="back faint" href="/shelves"><Icon name="chevron-left" size={16} /> Shelves</a>

	{#if loading}
		<Spinner label="Loading shelf" />
	{:else if error || !shelf}
		<Empty
			art={error && !error.missing ? 'failed' : 'lost'}
			title={error?.title ?? 'No shelf here'}
			hint={error?.hint}
		>
			{#if error && !error.missing}
				<button class="btn primary" onclick={load}>Try again</button>
			{/if}
			<a class="btn" class:primary={!error || error.missing} href="/shelves">All shelves</a>
		</Empty>
	{:else}
		<div class="head">
			<div class="title">
				{#if renaming}
					<form class="rename" onsubmit={rename}>
						<input
							class="input"
							bind:value={draftName}
							maxlength="60"
							aria-label="Shelf name"
							use:focusName
							onkeydown={(e) => {
								if (e.key === 'Escape') renaming = false;
							}}
						/>
						<button class="btn primary">Save</button>
						<button type="button" class="btn ghost" onclick={() => (renaming = false)}
							>Cancel</button
						>
					</form>
				{:else}
					<h1
						style:view-transition-name={morph.key === `shelf${shelf.id}`
							? `shelf-name-${shelf.id}`
							: undefined}
					>
						{shelf.name}
					</h1>
				{/if}
				<span class="faint sub">
					{#if shelf.kind === 'smart'}<Icon name="sparkle" size={14} /> Smart shelf ·{:else if watchlist}<Icon
							name="bookmark"
							size={14}
						/> Watchlist ·{/if}
					{plural(items.length, 'title')}
				</span>
			</div>
			<div class="controls">
				{#if shelf.kind === 'smart'}
					<button
						class="btn"
						class:primary={editingRules}
						onclick={() => (editingRules = !editingRules)}
					>
						<Icon name="filter" size={16} /> Rules
					</button>
				{:else}
					<button
						class="btn"
						class:primary={adding}
						onclick={() => (adding ? (adding = false) : openAdding())}
					>
						{#if adding}<Icon name="check" size={16} /> Done{:else}<Icon name="plus" size={16} /> Add{/if}
					</button>
					<button class="btn" onclick={() => (listing = true)}>
						<Icon name="list" size={16} /> Add a list
					</button>
					{#if items.length}
						<button class="btn" class:primary={editing} onclick={() => (editing = !editing)}>
							{#if editing}<Icon name="check" size={16} /> Done{:else}<Icon name="grip" size={16} /> Edit{/if}
						</button>
					{/if}
				{/if}
				<button class="btn ghost icon" aria-label="Rename" title="Rename" onclick={startRename}>
					<Icon name="pencil" size={16} />
				</button>
				<button
					class="btn ghost icon"
					aria-label="Delete shelf"
					title="Delete shelf"
					onclick={destroy}
				>
					<Icon name="trash" size={16} />
				</button>
			</div>
		</div>

		{#if listing}
			<ShelfFromList
				shelfId={id}
				shelfName={shelf.name}
				{onShelf}
				onclose={() => (listing = false)}
				ondone={(added) => {
					listing = false;
					toasts.push(added ? `Added ${plural(added, 'title')}` : 'All of it was here already');
					void load();
				}}
			/>
		{/if}

		{#if shelf.kind === 'smart' && editingRules}
			<ShelfRules rules={shelf.rules} onsave={saveRules} oncancel={() => (editingRules = false)} />
		{/if}

		{#if adding && watchlist}
			<section class="adder card" aria-label="Add anything">
				<input
					class="input"
					type="search"
					placeholder="Search films, shows and anime"
					aria-label="Search films, shows and anime"
					bind:value={pick}
					use:focusName
				/>
				{#if found.length}
					<ul>
						{#each found as result (`${result.kind}:${result.tmdb_id ?? ''}:${result.anilist_id ?? ''}`)}
							{@const here = result.id != null && onShelf.has(result.id)}
							<li>
								<button class="candidate" disabled={here} onclick={() => addFound(result)}>
									{#if result.poster_path}
										<img src={img(result.poster_path, 'w92')} alt="" loading="lazy" />
									{:else}
										<span class="thumb"></span>
									{/if}
									<span class="who">
										<strong>{result.name}</strong>
										<span class="faint"
											>{result.year ?? ''}{#if result.in_library}{result.year ? ' · ' : ''}In your
												library{/if}</span
										>
									</span>
									<Icon name={here ? 'check' : 'plus'} size={16} />
								</button>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="faint">
						{pick.trim().length < 2
							? 'Type a name. Nothing you add here goes into your library.'
							: searching
								? 'Searching'
								: 'Nothing by that name.'}
					</p>
				{/if}
			</section>
		{:else if adding}
			<section class="adder card" aria-label="Add from your library">
				<input
					class="input"
					placeholder="Filter your library"
					aria-label="Filter your library"
					bind:value={pick}
					use:focusName
				/>
				{#if candidates.length}
					<ul>
						{#each candidates as item (item.id)}
							<li>
								<button class="candidate" onclick={() => add(item)}>
									{#if item.poster_path}
										<img src={img(item.poster_path, 'w92')} alt="" loading="lazy" />
									{:else}
										<span class="thumb"></span>
									{/if}
									<span class="who">
										<strong>{item.name}</strong>
										<span class="faint">{year(item.release_date)}</span>
									</span>
									<Icon name="plus" size={16} />
								</button>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="faint">
						{library.length ? 'Everything that matches is already here.' : 'Loading your library'}
					</p>
				{/if}
			</section>
		{/if}

		{#if items.length === 0}
			{#if shelf.kind === 'smart'}
				<Empty title="Nothing matches yet" hint="Loosen the rules to let more in." art="search">
					{#if !editingRules}
						<button class="btn primary" onclick={() => (editingRules = true)}>Edit rules</button>
					{/if}
				</Empty>
			{:else}
				{#if !adding && watchlist}
					<Empty
						art="search"
						title="Nothing on this watchlist yet"
						hint="Add anything you want to see. It stays out of your library until you start it."
					>
						<button class="btn primary" onclick={openAdding}>Add titles</button>
						<button class="btn" onclick={() => (listing = true)}>Add a list</button>
					</Empty>
				{:else if !adding}
					<Empty
						title="Nothing on this shelf yet"
						hint="Pick from your library, or use Add to shelf on any title."
					>
						<button class="btn primary" onclick={openAdding}>Add from your library</button>
						<button class="btn" onclick={() => (listing = true)}>Add a list</button>
					</Empty>
				{/if}
			{/if}
		{:else if editing}
			<p class="faint hint">Drag to reorder.</p>
			<ol
				class="arrange"
				use:sortable={{ onmove: (from, to) => (items = moved(items, from, to)), onend: saveOrder }}
			>
				{#each items as item (item.id)}
					{@const src = img(item.poster_path, 'w342')}
					<li data-sort-item>
						<button class="handle" data-sort-handle aria-label="Move {item.name}">
							<span class="cover"
								>{#if src}<img {src} alt="" draggable="false" />{:else}<span class="faint"
										>{item.name}</span
									>{/if}</span
							>
							<span class="name">{item.name}</span>
						</button>
						<button class="remove hit" aria-label="Remove {item.name}" onclick={() => remove(item)}>
							<Icon name="x" size={14} />
						</button>
					</li>
				{/each}
			</ol>
		{:else}
			{#if watchlist && !adding}
				<WatchlistTally {items} {lengths} />
			{/if}
			{#if items.length > 1}
				<div class="segmented order" role="group" aria-label="Order">
					{#each sorts as s (s.value)}
						<button class:on={sortBy === s.value} onclick={() => sortTo(s.value)}>
							{s.label}
							{#if s.value === 'year' && sortBy === 'year'}
								<Icon name={oldest ? 'arrow-up' : 'arrow-down'} size={13} />
							{/if}
						</button>
					{/each}
				</div>
			{/if}
			<div class="case">
				<BookRows
					items={shown}
					{lengths}
					{watchlist}
					sheen={shelf.kind === 'smart'}
					{size}
					named={morph.key === `shelf${shelf.id}`}
					menuFor={(item) =>
						titleMenu(
							{ ...item, status: item.entry?.status ?? null, favourite: item.entry?.favourite },
							{ shelf: { ...shelf!, remove: () => remove(item) }, onchanged: load }
						)}
				/>
			</div>
		{/if}
	{/if}
</div>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.86rem;
		margin-bottom: 10px;
		text-decoration: none;
	}

	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 14px;
		margin-bottom: 22px;
	}

	/* The heading settles in, then its controls. */
	.title {
		animation: settle 460ms var(--spring) backwards;
	}

	.head .controls {
		animation: settle 460ms var(--spring) 90ms backwards;
	}

	@keyframes settle {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}

	.title {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.sub {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 0.86rem;
	}

	.rename {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	.rename .input {
		width: min(320px, 70vw);
	}

	.controls {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: center;
		min-width: 0;
	}

	.icon {
		padding: 10px;
	}

	.hint {
		font-size: 0.82rem;
		margin-bottom: 12px;
	}

	.arrange {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--poster-min), 1fr));
		gap: var(--gap);
	}

	.arrange li {
		position: relative;
	}

	.handle {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		text-align: inherit;
		border-radius: var(--radius-sm);
	}

	.cover {
		display: grid;
		place-items: center;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		border: 1px solid var(--line);
		font-size: 0.8rem;
		text-align: center;
		padding: 0;
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
		user-select: none;
	}

	.name {
		display: block;
		margin-top: 6px;
		font-size: 0.82rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.arrange :global([data-dragging] .cover) {
		box-shadow:
			0 0 0 2px var(--accent),
			var(--shadow-3);
	}

	.remove {
		position: absolute;
		top: -8px;
		right: -8px;
		width: 30px;
		height: 30px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev);
		color: var(--text);
		cursor: pointer;
		box-shadow: var(--shadow-2, 0 2px 6px rgba(0, 0, 0, 0.3));
	}

	.remove:hover {
		background: var(--bad);
		color: #fff;
	}

	.adder {
		padding: 14px;
		margin-bottom: 22px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.adder ul {
		list-style: none;
		margin: 0;
		padding: 0;
		max-height: 340px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.candidate {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 48px;
		padding: 6px 10px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		text-align: left;
		cursor: pointer;
	}

	.candidate:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.candidate:disabled {
		cursor: default;
		color: var(--text-faint);
	}

	.candidate img,
	.candidate .thumb {
		width: 28px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		object-fit: cover;
		background: var(--bg-elev-2);
		flex: none;
	}

	.candidate .who {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		font-size: 0.88rem;
	}

	.candidate .who .faint {
		font-size: 0.76rem;
	}

	.adder p {
		font-size: 0.86rem;
		padding: 4px 2px;
	}

	/* The bookcase the shelf's titles stand in, as on the Shelves page. */
	.case {
		padding: 0 30px 26px;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: linear-gradient(180deg, var(--shine), transparent 30%), var(--bg-elev);
		box-shadow:
			var(--shadow-3),
			inset 0 1px 0 var(--shine);
		backdrop-filter: var(--frost);
		-webkit-backdrop-filter: var(--frost);
		overflow: hidden;
	}

	.order {
		margin-bottom: 16px;
	}

	@media (max-width: 720px) {
		.case {
			padding: 0 12px 18px;
			border-radius: var(--radius);
		}
	}
</style>
