<script lang="ts">
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { SearchItem, Status } from '#lib/types';
	import { STATUS_LABEL } from '#lib/types';

	let term = $state('');
	let results = $state<SearchItem[]>([]);
	let trending = $state<SearchItem[]>([]);
	let loading = $state(false);
	let searched = $state(false);
	let adding = $state<string | null>(null);
	let input: HTMLInputElement | undefined = $state();

	$effect(() => {
		input?.focus();
		void api
			.trending()
			.then((r) => (trending = r.results))
			.catch(() => {
				// Trending is a nicety; a missing key or no network just means
				// the empty state stays plain.
			});
	});

	// Debounced as you type: 280 ms is long enough to avoid a request per
	// keystroke and short enough that it still feels live.
	$effect(() => {
		const q = term.trim();
		if (!q) {
			results = [];
			searched = false;
			return;
		}
		const timer = setTimeout(() => void run(q), 280);
		return () => clearTimeout(timer);
	});

	let token = 0;

	async function run(q: string) {
		const mine = ++token;
		loading = true;
		try {
			const res = await api.search(q, 1, settings.current.hideAnimation);
			if (mine !== token) return;
			results = res.results;
			searched = true;
		} catch (e) {
			if (mine === token) toasts.error(e);
		} finally {
			if (mine === token) loading = false;
		}
	}

	async function add(item: SearchItem, status: Status = settings.current.defaultStatus) {
		const key = `${item.kind}-${item.tmdb_id}`;
		adding = key;
		try {
			await api.track(item.kind, item.tmdb_id, { status });
			const mark = (list: SearchItem[]) =>
				list.map((r) =>
					r.kind === item.kind && r.tmdb_id === item.tmdb_id ? { ...r, in_library: status } : r
				);
			results = mark(results);
			trending = mark(trending);
			toasts.push(`${item.name} → ${STATUS_LABEL[status]}`);
		} catch (e) {
			toasts.error(e);
		} finally {
			adding = null;
		}
	}

	const shown = $derived(term.trim() ? results : trending);
	const heading = $derived(term.trim() ? 'Results' : 'Trending this week');
</script>

<svelte:head>
	<title>Search · Frog</title>
</svelte:head>


<div class="page">
	<h1 class="title">Search</h1>

	<div class="searchbar">
		<span class="icon" aria-hidden="true">⌕</span>
		<input
			bind:this={input}
			bind:value={term}
			class="field-input"
			placeholder="Films, shows, anything on TMDB"
			autocomplete="off"
			autocapitalize="off"
			spellcheck="false"
			aria-label="Search TMDB"
		/>
		{#if term}
			<button class="clear" aria-label="Clear search" onclick={() => (term = '')}>×</button>
		{/if}
	</div>

	{#if session.status && !session.status.tmdb_configured}
		<p class="notice">
			No TMDB key is set, so search cannot reach TMDB. Put <code>TMDB_API_KEY</code> in
			<code>.env</code> and restart the container.
		</p>
	{/if}

	{#if loading && !shown.length}
		<Spinner label="Searching" />
	{:else if !shown.length}
		{#if searched}
			<Empty title="Nothing found" hint="Try a shorter query, or the original-language title." />
		{:else}
			<Empty title="Find something to watch" hint="Start typing to search all of TMDB." />
		{/if}
	{:else}
		<div class="section-head">
			<h2>{heading}</h2>
			{#if loading}<span class="faint">updating…</span>{/if}
		</div>

		<Grid>
			{#each shown as item (`${item.kind}-${item.tmdb_id}`)}
				{@const key = `${item.kind}-${item.tmdb_id}`}
				<div class="result">
					<Poster
						kindBadge
						kind={item.kind}
						tmdbId={item.tmdb_id}
						name={item.name}
						posterPath={item.poster_path}
						year={item.year}
						status={item.in_library}
					/>

					{#if !item.in_library}
						<div class="quick">
							<button
								class="q"
								disabled={adding === key}
								title="Add to your library"
								aria-label="Add {item.name} to your library"
								onclick={() => add(item)}
							>
								＋
							</button>
							<button
								class="q"
								disabled={adding === key}
								title="Mark as watching"
								aria-label="Mark {item.name} as watching"
								onclick={() => add(item, 'watching')}
							>
								▶
							</button>
						</div>
					{/if}
				</div>
			{/each}
		</Grid>
	{/if}
</div>

<style>
	.title {
		margin-bottom: 18px;
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 16px;
		margin-bottom: 26px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		transition: border-color var(--fast) var(--ease);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.icon {
		font-size: 1.15rem;
		color: var(--text-faint);
	}

	.field-input {
		flex: 1;
		padding: 15px 0;
		background: none;
		border: none;
		outline: none;
		font-size: 1rem;
	}

	.clear {
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.3rem;
		cursor: pointer;
		padding: 0 4px;
	}

	.notice {
		padding: 12px 15px;
		margin-bottom: 22px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--warn) 12%, var(--bg-elev));
		border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
		font-size: 0.86rem;
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.92em;
	}

	.result {
		position: relative;
	}

	/* Quick-add sits over the art and only appears on hover or keyboard focus,
	   so the grid stays a wall of posters rather than a wall of buttons. */
	/* Always visible. A hover-only control is invisible on a laptop until you
	   go hunting for it, and absent on a touchscreen. */
	.quick {
		position: absolute;
		top: 7px;
		right: 7px;
		display: flex;
		gap: 5px;
		opacity: 0.92;
		transition: opacity var(--fast) var(--ease);
	}

	.result:hover .quick,
	.result:focus-within .quick {
		opacity: 1;
	}

	.q {
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1px solid rgba(255, 255, 255, 0.18);
		background: rgba(10, 10, 14, 0.75);
		backdrop-filter: blur(8px);
		color: #fff;
		font-size: 0.82rem;
		cursor: pointer;
	}

	.q:hover {
		background: var(--accent);
		border-color: transparent;
	}


</style>
