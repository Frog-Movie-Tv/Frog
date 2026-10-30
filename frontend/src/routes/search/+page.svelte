<script lang="ts">
	import { sameTitle, titleHref, titleKey } from '#lib/titles';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '#lib/components/Icon.svelte';
	import { api, frogId, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type {
		CharacterResult,
		MoreResults,
		PersonResult,
		PersonSummary,
		SearchItem,
		Status,
		StudioResult
	} from '#lib/types';
	import { STATUS_LABEL } from '#lib/types';

	type Tab = 'all' | 'titles' | 'people' | 'characters' | 'studios' | 'members';
	const TABS: { id: Tab; label: string }[] = [
		{ id: 'all', label: 'All' },
		{ id: 'titles', label: 'Films & shows' },
		{ id: 'people', label: 'People' },
		{ id: 'characters', label: 'Characters' },
		{ id: 'studios', label: 'Studios' },
		{ id: 'members', label: 'Members' }
	];
	const NONE: MoreResults = { people: [], studios: [], characters: [] };

	// The query and the tab live in the address, so Back from a title lands on
	// the same results and a search can be shared as a link.
	let term = $state(page.url.searchParams.get('q') ?? '');
	let tab = $state<Tab>(TABS.find((t) => t.id === page.url.searchParams.get('type'))?.id ?? 'all');
	let results = $state<SearchItem[]>([]);
	let more = $state<MoreResults>(NONE);
	let members = $state<PersonSummary[]>([]);
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
		void api
			.profiles()
			.then((r) => (members = r.profiles))
			.catch(() => {
				// Without profiles there are simply no members to find.
			});
	});

	// Debounced as you type: 280 ms is long enough to avoid a request per
	// keystroke and short enough that it still feels live.
	$effect(() => {
		const q = term.trim();
		if (!q) {
			results = [];
			more = NONE;
			searched = false;
			remember(
				'',
				untrack(() => tab)
			);
			return;
		}
		const timer = setTimeout(() => {
			remember(
				q,
				untrack(() => tab)
			);
			void run(q);
		}, 280);
		return () => clearTimeout(timer);
	});

	function remember(q: string, t: Tab) {
		const url = untrack(() => new URL(page.url.href));
		if (q) url.searchParams.set('q', q);
		else url.searchParams.delete('q');
		if (q && t !== 'all') url.searchParams.set('type', t);
		else url.searchParams.delete('type');
		if (url.search !== untrack(() => page.url.search)) {
			void goto(url, { shallow: true, replace: true });
		}
	}

	function choose(t: Tab) {
		tab = t;
		remember(term.trim(), t);
		window.scrollTo({ top: 0 });
	}

	let token = 0;

	async function run(q: string) {
		const mine = ++token;
		loading = true;
		// Titles and everything else arrive separately; neither waits on the
		// other, and one failing leaves the other on screen.
		const titles = api
			.search(q, 1, settings.current.hideAnimation)
			.then((res) => {
				if (mine === token) results = res.results;
			})
			.catch((e) => {
				if (mine === token) toasts.error(e);
			});
		const rest = api
			.searchMore(q)
			.then((res) => {
				if (mine === token) more = res;
			})
			.catch(() => {
				if (mine === token) more = NONE;
			});
		await Promise.all([titles, rest]);
		if (mine === token) {
			searched = true;
			loading = false;
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
			toasts.push(`${item.name} → ${STATUS_LABEL[status]}`);
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
	const counts = $derived<Record<Tab, number>>({
		all: 0,
		titles: results.length,
		people: more.people.length,
		characters: more.characters.length,
		studios: more.studios.length,
		members: foundMembers.length
	});
	const nothing = $derived(searched && TABS.every((t) => t.id === 'all' || counts[t.id] === 0));
	/** Someone typed a person's exact name: show people before titles. */
	const personFirst = $derived(
		!!more.people[0] &&
			plain(more.people[0].name) === plain(q) &&
			!results.some((r) => plain(r.name) === plain(q))
	);

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
		<span class="faint sub">{p.known_for[0] ?? p.department ?? ''}</span>
	</a>
{/snippet}

{#snippet characterCard(c: CharacterResult)}
	{@const src = characterImage(c)}
	<a class="face-card" href={characterHref(c)}>
		<span class="face portrait">
			{#if src}<img {src} alt="" loading="lazy" />{:else}<span>{c.name.slice(0, 1)}</span>{/if}
		</span>
		<strong>{c.name}</strong>
		{#if c.title.name}<span class="faint sub">{c.title.name}</span>{/if}
		{#if c.actor}<span class="faint sub">{c.actor.name}</span>{/if}
	</a>
{/snippet}

{#snippet studioCard(s: StudioResult)}
	{@const src = img(s.logo_path, 'w185')}
	<a class="studio" href="/studio/{s.id}">
		<span class="logo" class:blank={!src}>
			{#if src}<img {src} alt="" loading="lazy" />{:else}<span>{s.name.slice(0, 1)}</span>{/if}
		</span>
		<strong>{s.name}</strong>
		{#if s.country}<span class="faint sub">{s.country}</span>{/if}
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
		<span class="faint sub">@{m.username}</span>
	</a>
{/snippet}

{#snippet seeAll(id: Tab)}
	<button class="link" onclick={() => choose(id)}>See all</button>
{/snippet}

{#snippet section(id: Tab, title: string)}
	{#snippet more12()}{@render seeAll(id)}{/snippet}
	{#if counts[id]}
		<Rail {title} action={counts[id] > 12 ? more12 : undefined}>
			{#if id === 'people'}
				{#each more.people.slice(0, 12) as p (p.id)}{@render personCard(p)}{/each}
			{:else if id === 'characters'}
				{#each more.characters.slice(0, 12) as c, i (i)}{@render characterCard(c)}{/each}
			{:else if id === 'studios'}
				{#each more.studios.slice(0, 12) as s (s.id)}{@render studioCard(s)}{/each}
			{:else if id === 'members'}
				{#each foundMembers.slice(0, 12) as m (m.username)}{@render memberCard(m)}{/each}
			{/if}
		</Rail>
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
	</div>

	{#if q}
		<div class="chips tabs" role="tablist" aria-label="What to search">
			{#each TABS as t (t.id)}
				<button
					class="chip"
					role="tab"
					aria-selected={tab === t.id}
					class:on={tab === t.id}
					onclick={() => choose(t.id)}
				>
					{t.label}
					{#if t.id !== 'all' && searched}<span class="count">{counts[t.id]}</span>{/if}
				</button>
			{/each}
		</div>
	{/if}

	{#if session.status && !session.status.tmdb_configured}
		<p class="notice">
			Search is unavailable until metadata is connected.
			<a href="/about">More</a>
		</p>
	{/if}

	{#if !q}
		{#if trending.length}
			<div class="section-head"><h2>Trending this week</h2></div>
			<Grid>
				{#each trending as item (titleKey(item))}{@render titleCard(item)}{/each}
			</Grid>
		{:else}
			<Empty art="search" title="Find something to watch" hint="Start typing." />
		{/if}
	{:else if loading && !searched}
		<Spinner label="Searching" />
	{:else if nothing}
		<Empty
			art="search"
			title="Nothing found"
			hint="Try a shorter name, or the original language."
		/>
	{:else if tab === 'all'}
		{#if personFirst}{@render section('people', 'People')}{/if}
		{#if results.length}
			<div class="section-head">
				<h2>Films & shows</h2>
				{#if results.length > 12}{@render seeAll('titles')}{/if}
			</div>
			<div class="block">
				<Grid>
					{#each results.slice(0, 12) as item (titleKey(item))}
						{@render titleCard(item)}
					{/each}
				</Grid>
			</div>
		{/if}
		{#if !personFirst}{@render section('people', 'People')}{/if}
		{@render section('characters', 'Characters')}
		{@render section('studios', 'Studios')}
		{@render section('members', 'Members')}
	{:else if !counts[tab]}
		<Empty art="search" title="Nothing here" hint="Try All, or another name." />
	{:else if tab === 'titles'}
		<Grid>
			{#each results as item (titleKey(item))}{@render titleCard(item)}{/each}
		</Grid>
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

<style>
	.title {
		margin-bottom: 18px;
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 16px;
		margin-bottom: 16px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		transition: border-color var(--fast) var(--ease);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.field-input {
		flex: 1;
		min-width: 0;
		padding: 15px 0;
		background: none;
		border: none;
		outline: none;
		font-size: 1rem;
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

	/* Bled to the phone's edge like a rail, so the last tab scrolls into
	   view instead of being cut off at the gutter. */
	.tabs {
		margin: 0 calc(var(--page-pad) * -1) 26px;
		padding: 0 var(--page-pad) 2px;
	}

	.count {
		font-size: 0.74rem;
		font-weight: 600;
		opacity: 0.7;
	}

	.notice {
		padding: 12px 15px;
		margin-bottom: 22px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--warn) 12%, var(--bg-elev));
		border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
		font-size: 0.86rem;
	}

	.block {
		margin-bottom: 40px;
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
