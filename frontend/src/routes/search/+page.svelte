<script lang="ts">
	import { sameTitle, titleHref, titleKey } from '#lib/titles';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '#lib/components/Icon.svelte';
	import { api, frogId, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import FilterMenu from '#lib/components/FilterMenu.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import RailSkeleton from '#lib/components/RailSkeleton.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type {
		CharacterResult,
		MoreResults,
		PersonResult,
		PersonSummary,
		SearchFilters,
		SearchItem,
		Status,
		StudioResult
	} from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	type Tab = 'all' | 'movies' | 'shows' | 'people' | 'characters' | 'studios' | 'members';
	const TABS: { id: Tab; label: string }[] = [
		{ id: 'all', label: 'All' },
		{ id: 'movies', label: 'Films' },
		{ id: 'shows', label: 'Shows' },
		{ id: 'people', label: 'People' },
		{ id: 'characters', label: 'Characters' },
		{ id: 'studios', label: 'Studios' },
		{ id: 'members', label: 'Members' }
	];
	const NONE: MoreResults = { people: [], studios: [], characters: [] };
	const SORTS = [
		{ value: 'best', label: 'Best match' },
		{ value: 'popular', label: 'Popular' },
		{ value: 'rating', label: 'Top rated' },
		{ value: 'newest', label: 'Newest' },
		{ value: 'oldest', label: 'Oldest' }
	];
	const SCORES = [
		{ value: '0', label: 'Any' },
		{ value: '50', label: '5+' },
		{ value: '60', label: '6+' },
		{ value: '70', label: '7+' },
		{ value: '80', label: '8+' },
		{ value: '90', label: '9+' }
	];
	const WHERE: { id: Status | 'none'; label: string }[] = [
		{ id: 'none', label: 'Not in my library' },
		...STATUSES.map((s) => ({ id: s, label: STATUS_LABEL[s] }))
	];

	// The query, the tab and the filters live in the address, so Back from a
	// title lands on the same results and a search can be shared as a link.
	const start = page.url.searchParams;
	const year = (v: string | null) => (v && /^\d{4}$/.test(v) ? Number(v) : null);
	const parts = (v: string | null) => (v ? v.split(',').filter(Boolean) : []);
	let term = $state(start.get('q') ?? '');
	let tab = $state<Tab>(TABS.find((t) => t.id === start.get('type'))?.id ?? 'all');
	let genres = $state<string[]>(parts(start.get('genres')));
	let fromYear = $state<number | null>(year(start.get('from')));
	let toYear = $state<number | null>(year(start.get('to')));
	let minScore = $state(start.get('score') ?? '0');
	let statuses = $state<(Status | 'none')[]>(parts(start.get('status')) as (Status | 'none')[]);
	let sort = $state(start.get('sort') ?? 'best');
	let panelOpen = $state(false);
	let allGenres = $state<string[]>([]);

	let results = $state<SearchItem[]>([]);
	let nextPage = $state<number | null>(null);
	let personFirst = $state(false);
	let more = $state<MoreResults>(NONE);
	let moreReady = $state(false);
	let members = $state<PersonSummary[]>([]);
	let trending = $state<SearchItem[]>([]);
	let loading = $state(false);
	let loadingMore = $state(false);
	let searched = $state(false);
	let adding = $state<string | null>(null);
	let input: HTMLInputElement | undefined = $state();

	const filterCount = $derived(
		(genres.length ? 1 : 0) +
			(fromYear || toYear ? 1 : 0) +
			(minScore !== '0' ? 1 : 0) +
			(statuses.length ? 1 : 0) +
			(sort !== 'best' ? 1 : 0)
	);

	$effect(() => {
		input?.focus();
		void api
			.trending()
			.then((r) => (trending = r.results))
			.catch(() => {
				// Trending is a nicety; a missing key or no network just means
				// the empty state stays plain.
			});
		void api
			.profiles()
			.then((r) => (members = r.profiles))
			.catch(() => {
				// Without profiles there are simply no members to find.
			});
		void api
			.searchGenres()
			.then((r) => (allGenres = r.genres))
			.catch(() => {
				// Without the list, genres are simply not offered.
			});
		if (untrack(() => filterCount)) panelOpen = true;
	});

	function currentFilters(): SearchFilters {
		return {
			genres: genres.length ? genres : undefined,
			from_year: fromYear,
			to_year: toYear,
			min_score: minScore === '0' ? null : Number(minScore),
			statuses: statuses.length ? statuses : undefined,
			sort: sort === 'best' ? null : (sort as SearchFilters['sort'])
		};
	}

	// Debounced as you type: 280 ms is long enough to avoid a request per
	// keystroke and short enough that it still feels live.
	$effect(() => {
		const q = term.trim();
		const filtered = filterCount > 0;
		// Read here, so a change to any of them searches again.
		void [genres.length, fromYear, toYear, minScore, statuses.length, sort];
		if (!q && !filtered) {
			token++;
			results = [];
			nextPage = null;
			personFirst = false;
			more = NONE;
			moreReady = false;
			searched = false;
			loading = false;
			remember();
			return;
		}
		const timer = setTimeout(() => {
			remember();
			void run(q);
		}, 280);
		return () => clearTimeout(timer);
	});

	function remember() {
		const url = untrack(() => new URL(page.url.href));
		const set = (key: string, value: string | number | null | undefined) => {
			if (value) url.searchParams.set(key, String(value));
			else url.searchParams.delete(key);
		};
		untrack(() => {
			set('q', term.trim());
			set('type', tab === 'all' ? null : tab);
			set('genres', genres.join(','));
			set('from', fromYear);
			set('to', toYear);
			set('score', minScore === '0' ? null : minScore);
			set('status', statuses.join(','));
			set('sort', sort === 'best' ? null : sort);
		});
		if (url.search !== untrack(() => page.url.search)) {
			void goto(url, { shallow: true, replace: true });
		}
	}

	function choose(t: Tab) {
		tab = t;
		remember();
		window.scrollTo({ top: 0 });
	}

	function toggleGenre(g: string) {
		genres = genres.includes(g) ? genres.filter((x) => x !== g) : [...genres, g];
	}

	function toggleStatus(s: Status | 'none') {
		statuses = statuses.includes(s) ? statuses.filter((x) => x !== s) : [...statuses, s];
	}

	function clearFilters() {
		genres = [];
		fromYear = null;
		toYear = null;
		minScore = '0';
		statuses = [];
		sort = 'best';
	}

	/** A year applies as soon as all four digits are in; clearing it clears it. */
	function yearInput(value: string, was: number | null): number | null {
		const v = value.trim();
		if (!v) return null;
		const n = Number(v);
		if (/^\d{4}$/.test(v) && n > 1800 && n < 2200) return n;
		return was;
	}

	let token = 0;

	async function run(q: string) {
		const mine = ++token;
		loading = true;
		// Titles and everything else arrive separately; neither waits on the
		// other, and one failing leaves the other on screen.
		const titles = api
			.search(q, 1, settings.current.hideAnimation, currentFilters())
			.then((res) => {
				if (mine !== token) return;
				results = res.results;
				nextPage = res.next_page;
				personFirst = !!res.person_first;
				searched = true;
			})
			.catch((e) => {
				if (mine === token) toasts.error(e);
			});
		const rest = q
			? api
					.searchMore(q)
					.then((res) => {
						if (mine === token) more = res;
					})
					.catch(() => {
						if (mine === token) more = NONE;
					})
			: Promise.resolve((more = NONE));
		await titles;
		await rest;
		if (mine === token) {
			searched = true;
			moreReady = true;
			loading = false;
		}
	}

	async function loadMore() {
		if (!nextPage) return;
		const mine = token;
		loadingMore = true;
		try {
			const res = await api.search(
				term.trim(),
				nextPage,
				settings.current.hideAnimation,
				currentFilters()
			);
			if (mine !== token) return;
			const seen = new Set(results.map(titleKey));
			results = [...results, ...res.results.filter((r) => !seen.has(titleKey(r)))];
			nextPage = res.next_page;
		} catch (e) {
			toasts.error(e);
		} finally {
			loadingMore = false;
		}
	}

	async function add(item: SearchItem, status: Status = settings.current.defaultStatus) {
		const key = titleKey(item);
		adding = key;
		try {
			const id = await frogId(item);
			await api.track(id, { status });
			const mark = (list: SearchItem[]) =>
				list.map((r) => (sameTitle(r, item) ? { ...r, id, in_library: status } : r));
			results = mark(results);
			trending = mark(trending);
			toasts.push(`Added ${item.name} to ${STATUS_LABEL[status]}`);
		} catch (e) {
			toasts.error(e);
		} finally {
			adding = null;
		}
	}

	const plain = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');

	const q = $derived(term.trim());
	const foundMembers = $derived(
		q
			? members.filter(
					(m) =>
						plain(m.username).includes(plain(q)) || plain(m.display_name ?? '').includes(plain(q))
				)
			: []
	);
	const films = $derived(results.filter((r) => r.kind === 'movie'));
	const shows = $derived(results.filter((r) => r.kind === 'tv'));
	const counts = $derived<Record<Tab, number>>({
		all: 0,
		movies: films.length,
		shows: shows.length,
		people: more.people.length,
		characters: more.characters.length,
		studios: more.studios.length,
		members: foundMembers.length
	});
	const asking = $derived(!!q || filterCount > 0);
	/** People, characters, studios and members are found by name; filters
	 *  alone only find films and shows. */
	const byName = (t: Tab) => !['all', 'movies', 'shows'].includes(t);
	$effect(() => {
		if (!q && byName(tab)) untrack(() => choose('all'));
	});
	/** Trending, narrowed to films or shows when that tab is open. */
	const trendingShown = $derived(
		tab === 'movies'
			? trending.filter((t) => t.kind === 'movie')
			: tab === 'shows'
				? trending.filter((t) => t.kind === 'tv')
				: trending
	);

	/** What each filter menu says it is set to. */
	const genreValue = $derived(
		genres.length === 0
			? null
			: genres.length === 1
				? genres[0]
				: `${genres[0]} +${genres.length - 1}`
	);
	const yearValue = $derived(
		fromYear && toYear
			? fromYear === toYear
				? String(fromYear)
				: `${fromYear}–${toYear}`
			: fromYear
				? `${fromYear} on`
				: toYear
					? `to ${toYear}`
					: null
	);
	const scoreValue = $derived(
		minScore === '0' ? null : (SCORES.find((s) => s.value === minScore)?.label ?? null)
	);
	const statusValue = $derived(
		statuses.length === 0
			? null
			: statuses.length === 1
				? (WHERE.find((w) => w.id === statuses[0])?.label ?? null)
				: `${statuses.length} chosen`
	);
	const sortValue = $derived(
		sort === 'best' ? null : (SORTS.find((s) => s.value === sort)?.label ?? null)
	);
	const nothing = $derived(
		searched && !loading && TABS.every((t) => t.id === 'all' || counts[t.id] === 0)
	);
	/** What a person does, in a word. */
	const JOBS: Record<string, string> = {
		Acting: 'Actor',
		Directing: 'Director',
		Writing: 'Writer',
		Production: 'Producer',
		Sound: 'Composer',
		Creator: 'Creator'
	};
	const job = (p: PersonResult) => (p.department ? (JOBS[p.department] ?? p.department) : 'Person');

	function characterHref(c: CharacterResult): string {
		const t = c.title;
		return titleHref(t) ?? `/search?q=${encodeURIComponent(t.name ?? c.name)}`;
	}

	function characterImage(c: CharacterResult): string | null {
		if (!c.image) return null;
		return c.image.source === 'anilist'
			? `/img/anilist/${c.image.path}`
			: img(c.image.path, 'w185');
	}
</script>

<svelte:head>
	<title>Search · Frog</title>
</svelte:head>

{#snippet titleCard(item: SearchItem)}
	{@const key = titleKey(item)}
	<div class="result">
		<Poster
			kindBadge
			kind={item.kind}
			ref={item}
			name={item.name}
			posterPath={item.poster_path}
			fallbackPath={item.backdrop_path}
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
					<Icon name="plus" size={15} />
				</button>
				<button
					class="q secondary"
					disabled={adding === key}
					title="Mark as watching"
					aria-label="Mark {item.name} as watching"
					onclick={() => add(item, 'watching')}
				>
					<Icon name="play" size={13} filled />
				</button>
			</div>
		{/if}
	</div>
{/snippet}

{#snippet personCard(p: PersonResult)}
	{@const src = img(p.profile_path, 'w185')}
	<a class="face-card" href="/person/{p.id}">
		<span class="face">
			{#if src}<img {src} alt="" loading="lazy" />{:else}<span>{p.name.slice(0, 1)}</span>{/if}
		</span>
		<strong>{p.name}</strong>
		<span class="faint sub">{job(p)}{p.known_for[0] ? ` · ${p.known_for[0]}` : ''}</span>
	</a>
{/snippet}

{#snippet characterCard(c: CharacterResult)}
	{@const src = characterImage(c)}
	<a class="face-card" href={characterHref(c)}>
		<span class="face portrait">
			{#if src}<img {src} alt="" loading="lazy" />{:else}<span>{c.name.slice(0, 1)}</span>{/if}
		</span>
		<strong>{c.name}</strong>
		<span class="faint sub">Character{c.title.name ? ` in ${c.title.name}` : ''}</span>
		{#if c.actor}<span class="faint sub">Played by {c.actor.name}</span>{/if}
	</a>
{/snippet}

{#snippet studioCard(s: StudioResult)}
	{@const src = img(s.logo_path, 'w185')}
	<a class="studio" href="/studio/{s.id}">
		<span class="logo" class:blank={!src}>
			{#if src}<img {src} alt="" loading="lazy" />{:else}<span>{s.name.slice(0, 1)}</span>{/if}
		</span>
		<strong>{s.name}</strong>
		<span class="faint sub">Studio{s.country ? ` · ${s.country}` : ''}</span>
	</a>
{/snippet}

{#snippet memberCard(m: PersonSummary)}
	<a class="face-card member" href="/people/{encodeURIComponent(m.username)}">
		<Avatar
			src={m.avatar ? `/api/profiles/${encodeURIComponent(m.username)}/images/avatar` : null}
			name={m.username}
			size={96}
		/>
		<strong>{m.display_name ?? m.username}</strong>
		<span class="faint sub">Member · @{m.username}</span>
	</a>
{/snippet}

{#snippet seeAll(id: Tab)}
	<button class="link" onclick={() => choose(id)}>See all</button>
{/snippet}

{#snippet section(id: Tab, title: string)}
	{#snippet more12()}{@render seeAll(id)}{/snippet}
	{#if counts[id]}
		<Rail {title} action={counts[id] > 12 ? more12 : undefined}>
			{#if id === 'movies'}
				{#each films.slice(0, 12) as item (titleKey(item))}{@render titleCard(item)}{/each}
			{:else if id === 'shows'}
				{#each shows.slice(0, 12) as item (titleKey(item))}{@render titleCard(item)}{/each}
			{:else if id === 'people'}
				{#each more.people.slice(0, 12) as p (p.id)}{@render personCard(p)}{/each}
			{:else if id === 'characters'}
				{#each more.characters.slice(0, 12) as c, i (i)}{@render characterCard(c)}{/each}
			{:else if id === 'studios'}
				{#each more.studios.slice(0, 12) as s (s.id)}{@render studioCard(s)}{/each}
			{:else if id === 'members'}
				{#each foundMembers.slice(0, 12) as m (m.username)}{@render memberCard(m)}{/each}
			{/if}
		</Rail>
	{:else if id === 'people' && !moreReady}
		<RailSkeleton />
	{/if}
{/snippet}

{#snippet showMore()}
	{#if nextPage}
		<div class="more">
			<button class="btn" disabled={loadingMore} onclick={loadMore}>
				{loadingMore ? 'Loading…' : 'More results'}
			</button>
		</div>
	{/if}
{/snippet}

<div class="page">
	<h1 class="title">Search</h1>

	<div class="searchbar">
		<Icon name="search" size={19} />
		<input
			bind:this={input}
			bind:value={term}
			class="field-input"
			placeholder="Films, shows, people, characters, studios"
			autocomplete="off"
			autocapitalize="off"
			spellcheck="false"
			aria-label="Search"
		/>
		{#if term}
			<button class="clear" aria-label="Clear search" onclick={() => (term = '')}>
				<Icon name="x" size={17} />
			</button>
		{/if}
		<button
			class="filter-toggle"
			class:on={panelOpen || filterCount > 0}
			aria-expanded={panelOpen}
			aria-controls="search-filters"
			onclick={() => (panelOpen = !panelOpen)}
		>
			<Icon name="filter" size={16} />
			<span>Filters</span>
			{#if filterCount}<span class="badge">{filterCount}</span>{/if}
		</button>
	</div>

	{#if panelOpen}
		<div class="filterbar" id="search-filters" aria-label="Filters">
			<FilterMenu label="Genre" value={genreValue} wide>
				<div class="choices">
					{#each allGenres as g (g)}
						<button class="chip" class:on={genres.includes(g)} onclick={() => toggleGenre(g)}>
							{g}
						</button>
					{/each}
				</div>
			</FilterMenu>

			<FilterMenu label="Year" value={yearValue}>
				<div class="years">
					<label class="field">
						<span>From</span>
						<input
							class="input"
							inputmode="numeric"
							placeholder="Any"
							value={fromYear ?? ''}
							oninput={(e) => (fromYear = yearInput(e.currentTarget.value, fromYear))}
						/>
					</label>
					<label class="field">
						<span>To</span>
						<input
							class="input"
							inputmode="numeric"
							placeholder="Any"
							value={toYear ?? ''}
							oninput={(e) => (toYear = yearInput(e.currentTarget.value, toYear))}
						/>
					</label>
				</div>
			</FilterMenu>

			<FilterMenu label="Rated" value={scoreValue}>
				<div class="list" role="radiogroup" aria-label="Rated at least">
					{#each SCORES as o (o.value)}
						<button
							class="opt"
							role="radio"
							aria-checked={minScore === o.value}
							onclick={() => (minScore = o.value)}
						>
							<span class="tick"
								>{#if minScore === o.value}<Icon name="check" size={15} />{/if}</span
							>
							{o.value === '0' ? 'Any score' : `${o.label} on TMDB`}
						</button>
					{/each}
				</div>
			</FilterMenu>

			<FilterMenu label="In my library" value={statusValue}>
				<div class="list">
					{#each WHERE as w (w.id)}
						<button
							class="opt"
							role="menuitemcheckbox"
							aria-checked={statuses.includes(w.id)}
							onclick={() => toggleStatus(w.id)}
						>
							<span class="tick box" class:on={statuses.includes(w.id)}
								>{#if statuses.includes(w.id)}<Icon name="check" size={13} />{/if}</span
							>
							{w.label}
						</button>
					{/each}
				</div>
			</FilterMenu>

			<FilterMenu label="Sort" value={sortValue}>
				<div class="list" role="radiogroup" aria-label="Sort by">
					{#each SORTS as o (o.value)}
						<button
							class="opt"
							role="radio"
							aria-checked={sort === o.value}
							onclick={() => (sort = o.value)}
						>
							<span class="tick"
								>{#if sort === o.value}<Icon name="check" size={15} />{/if}</span
							>
							{o.label}
						</button>
					{/each}
				</div>
			</FilterMenu>

			{#if filterCount}
				<button class="link clear-all" onclick={clearFilters}>Clear filters</button>
			{/if}
		</div>
	{/if}

	<div class="tabbar" role="tablist" aria-label="What to search">
		{#each TABS as t (t.id)}
			<button
				class="tab"
				role="tab"
				aria-selected={tab === t.id}
				class:on={tab === t.id}
				disabled={!q && byName(t.id)}
				title={!q && byName(t.id) ? 'Type a name to find these' : undefined}
				onclick={() => choose(t.id)}
			>
				{t.label}
				{#if t.id !== 'all' && searched && (q || !byName(t.id))}
					<span class="count">{counts[t.id]}</span>
				{/if}
			</button>
		{/each}
	</div>

	<div class="body">
		<div class="results">
			{#if session.status && !session.status.tmdb_configured}
				<p class="notice">
					Search is unavailable until metadata is connected.
					<a href="/about">More</a>
				</p>
			{/if}

			{#if !asking || !searched}
				<!-- What was on screen stays until the first answer replaces it, so a
		     search starting moves nothing. -->
				<div class:dim={asking}>
					{#if trendingShown.length}
						<div class="section-head"><h2>Trending this week</h2></div>
						<Grid>
							{#each trendingShown as item (titleKey(item))}{@render titleCard(item)}{/each}
						</Grid>
					{:else if asking}
						<Spinner label="Searching" />
					{:else}
						<Empty art="search" title="Find something to watch" hint="Start typing." />
					{/if}
				</div>
			{:else if nothing}
				<Empty
					art="search"
					title="Nothing found"
					hint={filterCount
						? 'Try fewer filters, or a shorter name.'
						: 'Try a shorter name, or the original language.'}
				/>
			{:else if tab === 'all'}
				{#if personFirst}{@render section('people', 'People')}{/if}
				{@render section('movies', 'Films')}
				{@render section('shows', 'Shows')}
				{#if !personFirst && q}{@render section('people', 'People')}{/if}
				{@render section('characters', 'Characters')}
				{@render section('studios', 'Studios')}
				{@render section('members', 'Members')}
				{@render showMore()}
			{:else if !counts[tab]}
				{#if moreReady || tab === 'movies' || tab === 'shows'}
					<Empty art="search" title="Nothing here" hint="Try All, or another name." />
				{/if}
			{:else if tab === 'movies' || tab === 'shows'}
				<Grid>
					{#each tab === 'movies' ? films : shows as item (titleKey(item))}
						{@render titleCard(item)}
					{/each}
				</Grid>
				{@render showMore()}
			{:else}
				<div class="cards">
					{#if tab === 'people'}
						{#each more.people as p (p.id)}{@render personCard(p)}{/each}
					{:else if tab === 'characters'}
						{#each more.characters as c, i (i)}{@render characterCard(c)}{/each}
					{:else if tab === 'studios'}
						{#each more.studios as s (s.id)}{@render studioCard(s)}{/each}
					{:else if tab === 'members'}
						{#each foundMembers as m (m.username)}{@render memberCard(m)}{/each}
					{/if}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	/* Tall enough that the scrollbar never comes and goes with the results. */
	.page {
		min-height: 100dvh;
	}

	.title {
		font-size: clamp(1.7rem, 3vw, 2.25rem);
		letter-spacing: -0.025em;
		margin-bottom: 20px;
	}

	.tabbar {
		display: flex;
		gap: 4px;
		margin-bottom: 24px;
		border-bottom: 1px solid var(--line);
		overflow-x: auto;
		scrollbar-width: none;
	}

	.tabbar::-webkit-scrollbar {
		display: none;
	}

	.tab {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: 7px;
		flex: none;
		padding: 10px 12px 12px;
		border: 0;
		background: none;
		color: var(--text-faint);
		font-size: 0.92rem;
		font-weight: 600;
		cursor: pointer;
		transition: color var(--fast) var(--ease);
	}

	.tab:hover {
		color: var(--text);
	}

	.tab.on {
		color: var(--text);
	}

	.tab:disabled {
		opacity: 0.45;
		cursor: default;
	}

	.tab.on::after {
		content: '';
		position: absolute;
		left: 10px;
		right: 10px;
		bottom: -1px;
		height: 2px;
		border-radius: 2px;
		background: var(--accent);
	}

	.tab .count {
		font-size: 0.74rem;
		font-weight: 600;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.filterbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin: -4px 0 18px;
	}

	.clear-all {
		margin-left: 4px;
	}

	.choices {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.years {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
		width: 240px;
		max-width: 100%;
	}

	.list {
		display: grid;
		gap: 2px;
		min-width: 200px;
	}

	.opt {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 10px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		font-size: 0.9rem;
		text-align: left;
		cursor: pointer;
	}

	.opt:hover {
		background: var(--bg-elev-2);
	}

	.tick {
		display: grid;
		place-items: center;
		flex: none;
		width: 18px;
		height: 18px;
		color: var(--accent);
	}

	.tick.box {
		border: 1.5px solid var(--line-strong);
		border-radius: 5px;
	}

	.tick.box.on {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--accent-ink);
	}

	@media (max-width: 760px) {
		/* One row that scrolls sideways, like the tabs under it. */
		.filterbar {
			flex-wrap: nowrap;
			overflow-x: auto;
			scrollbar-width: none;
			margin-inline: calc(var(--page-pad) * -1);
			padding: 2px var(--page-pad);
		}

		.filterbar::-webkit-scrollbar {
			display: none;
		}

		.clear-all {
			flex: none;
		}
	}

	.results {
		min-width: 0;
	}

	.dim {
		opacity: 0.55;
		transition: opacity var(--fast) var(--ease);
	}

	.filter-toggle {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		padding: 7px 12px;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		background: none;
		color: var(--text-dim);
		font: inherit;
		font-size: 0.84rem;
		font-weight: 600;
		cursor: pointer;
	}

	.filter-toggle.on {
		color: var(--text);
		border-color: var(--accent);
	}

	.badge {
		display: grid;
		place-items: center;
		min-width: 18px;
		height: 18px;
		padding: 0 5px;
		border-radius: 9px;
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.72rem;
	}

	.field {
		display: grid;
		gap: 6px;
		min-width: 0;
	}

	.more {
		display: flex;
		justify-content: center;
		margin: 8px 0 32px;
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 10px 0 20px;
		margin-bottom: 18px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		box-shadow: var(--shadow-2);
		transition: border-color var(--fast) var(--ease);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.field-input {
		flex: 1;
		min-width: 0;
		padding: 17px 0;
		background: none;
		border: none;
		outline: none;
		font-size: 1.1rem;
	}

	.clear {
		display: grid;
		place-items: center;
		background: none;
		border: none;
		color: var(--text-faint);
		cursor: pointer;
		padding: 4px;
	}

	.clear:hover {
		color: var(--text);
	}

	@media (max-width: 760px) {
		/* Bled to the phone's edge like a rail, so the last tab scrolls into
		   view instead of being cut off at the gutter. */
		.tabbar {
			margin-inline: calc(var(--page-pad) * -1);
			padding-inline: calc(var(--page-pad) - 10px);
		}
	}

	.notice {
		padding: 12px 15px;
		margin-bottom: 22px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--warn) 12%, var(--bg-elev));
		border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
		font-size: 0.86rem;
	}

	.link {
		background: none;
		border: none;
		padding: 0;
		color: var(--accent);
		font: inherit;
		font-size: 0.86rem;
		font-weight: 600;
		cursor: pointer;
	}

	.result {
		position: relative;
	}

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

	/* Three posters across a phone leave room for one button beside the
	   badge; Watching is a tap away on the title page. */
	@media (max-width: 720px) {
		.q.secondary {
			display: none;
		}
	}

	/* A whole category: the same cards as the rails, wrapped. Fixed widths,
	   never 1fr, so three results do not stretch across the page. */
	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, var(--poster-min));
		gap: calc(var(--gap) * 1.4) var(--gap);
		justify-content: start;
	}

	.face-card,
	.studio {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 3px;
		text-align: center;
		min-width: 0;
	}

	.face-card strong,
	.studio strong {
		margin-top: 7px;
		font-size: 0.86rem;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.sub {
		font-size: 0.74rem;
		line-height: 1.25;
		max-width: 100%;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.face {
		display: grid;
		place-items: center;
		width: 100%;
		aspect-ratio: 1;
		border-radius: 50%;
		overflow: hidden;
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-family: var(--font-display);
		font-size: 2rem;
		transition: transform var(--fast) var(--ease);
	}

	/* Characters are pictures of people drawn or played, taller than wide. */
	.face.portrait {
		aspect-ratio: 3 / 4;
		border-radius: var(--radius);
	}

	.face img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.face-card:hover .face,
	.studio:hover .logo {
		transform: translateY(-2px);
	}

	.member :global(.avatar) {
		width: 100% !important;
		height: auto !important;
		aspect-ratio: 1;
		font-size: 2rem !important;
	}

	/* Logos are drawn for white paper, often dark on transparent, so they sit
	   on a light tile in every theme. */
	.logo {
		display: grid;
		place-items: center;
		width: 100%;
		aspect-ratio: 3 / 2;
		padding: 14px;
		border-radius: var(--radius);
		background: #f6f4ec;
		border: 1px solid var(--line);
		transition: transform var(--fast) var(--ease);
	}

	.logo img {
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
	}

	.logo.blank {
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-family: var(--font-display);
		font-size: 1.8rem;
	}
</style>
