<script lang="ts">
	import { api, type LibraryQuery } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { year } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	let items = $state<LibraryItem[]>([]);
	let continuing = $state<LibraryItem[]>([]);
	let loading = $state(true);
	let failed = $state(false);

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

	/** Genres present in the library, so the filter only offers real options. */
	const genres = $derived(
		[...new Set(items.flatMap((i) => i.genres))].sort((a, b) => a.localeCompare(b))
	);

	const counts = $derived.by(() => {
		const out: Partial<Record<Status, number>> = {};
		for (const i of items) {
			if (i.entry) out[i.entry.status] = (out[i.entry.status] ?? 0) + 1;
		}
		return out;
	});

	// Re-query whenever any control changes. The backend does the filtering so
	// this stays correct past the page size.
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
			const [all, watching] = await Promise.all([
				api.library(query),
				api.library({ status: 'watching', sort: 'last_watched', limit: 20 })
			]);
			if (mine !== token) return;
			items = all.items;
			continuing = watching.items;
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
</script>

<div class="page">
	{#if continuing.length && !filtered}
		<Rail title="Continue watching">
			{#each continuing as item (item.id)}
				<Poster
					kind={item.kind}
					tmdbId={item.tmdb_id}
					name={item.name}
					posterPath={item.poster_path}
					year={year(item.release_date)}
					rating={item.entry?.rating ?? null}
					watched={item.watched_episodes}
					total={item.total_episodes}
				/>
			{/each}
		</Rail>
	{/if}

	<div class="section-head">
		<h1>Library</h1>
		<span class="faint">{items.length} {items.length === 1 ? 'title' : 'titles'}</span>
	</div>

	<div class="controls">
		<div class="chips" role="group" aria-label="Status filter">
			<button class="chip" class:on={status === 'all'} onclick={() => (status = 'all')}>
				All
			</button>
			{#each STATUSES as s (s)}
				<button class="chip" class:on={status === s} onclick={() => (status = s)}>
					{STATUS_LABEL[s]}
					{#if counts[s]}<span class="n">{counts[s]}</span>{/if}
				</button>
			{/each}
		</div>

		<div class="row">
			<input class="input search" placeholder="Filter by name" bind:value={term} />

			<select class="input" bind:value={kind} aria-label="Type">
				<option value="all">Films &amp; shows</option>
				<option value="movie">Films</option>
				<option value="tv">Shows</option>
			</select>

			<select class="input" bind:value={genre} aria-label="Genre">
				<option value="">Any genre</option>
				{#each genres as g (g)}
					<option value={g}>{g}</option>
				{/each}
			</select>

			<select class="input" bind:value={sort} aria-label="Sort by">
				<option value="added">Recently added</option>
				<option value="updated">Recently updated</option>
				<option value="last_watched">Last watched</option>
				<option value="name">Name</option>
				<option value="rating">My rating</option>
				<option value="release">Release date</option>
			</select>

			<button
				class="btn ghost"
				aria-label="Reverse order"
				title="Reverse order"
				onclick={() => (order = order === 'desc' ? 'asc' : 'desc')}
			>
				{order === 'desc' ? '↓' : '↑'}
			</button>

			<button
				class="btn ghost"
				class:on={favourite}
				aria-pressed={favourite}
				title="Favourites only"
				onclick={() => (favourite = !favourite)}
			>
				{favourite ? '♥' : '♡'}
			</button>
		</div>
	</div>

	{#if loading && !items.length}
		<Spinner label="Loading your library" />
	{:else if failed && !items.length}
		<Empty title="Could not load your library" hint="The server did not answer. Try again." />
	{:else if !items.length}
		<Empty
			title={filtered ? 'Nothing matches those filters' : 'Your library is empty'}
			hint={filtered
				? 'Loosen a filter and it will come back.'
				: 'Search for a film or show and add it. Everything you track lives here.'}
		>
			{#if filtered}
				<button class="btn" onclick={reset}>Clear filters</button>
			{:else}
				<a class="btn primary" href="/search">Find something to watch</a>
			{/if}
		</Empty>
	{:else}
		<Grid>
			{#each items as item (item.id)}
				<Poster
					morph
					kind={item.kind}
					tmdbId={item.tmdb_id}
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
	{/if}
</div>

<style>
	.controls {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 26px;
	}

	.chips {
		display: flex;
		gap: 7px;
		overflow-x: auto;
		scrollbar-width: none;
		padding-bottom: 2px;
	}

	.chips::-webkit-scrollbar {
		display: none;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 7px 14px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.84rem;
		font-weight: 600;
		cursor: pointer;
		white-space: nowrap;
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.chip:hover {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.chip.on {
		background: var(--accent);
		border-color: transparent;
		color: var(--accent-ink);
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
</style>
