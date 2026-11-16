<script lang="ts">
	import { titleHref, titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import ActivityCard from '#lib/components/ActivityCard.svelte';
	import ContinueCard from '#lib/components/ContinueCard.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import HomeHero, { slidesOf } from '#lib/components/HomeHero.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import RailSkeleton from '#lib/components/RailSkeleton.svelte';
	import ShelfCard from '#lib/components/ShelfCard.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { airTime, relativeDate, year } from '#lib/format';
	import { episodeRange, firstMoment } from '#lib/thisweek';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { shelfMenu, titleMenu } from '#lib/menus';
	import type {
		ActivityItem,
		BecauseRow,
		FeaturedTitle,
		HomeData,
		LibraryItem,
		SearchItem,
		UpNextEntry
	} from '#lib/types';

	let data = $state<HomeData | null>(null);
	let loading = $state(true);
	let failed = $state<Failure | null>(null);
	let because = $state<BecauseRow[]>([]);
	/** Trending titles for the hero, asked for apart: they may need TMDB. */
	let featured = $state<FeaturedTitle[]>([]);
	let activity = $state<ActivityItem[]>([]);

	$effect(() => {
		if (!session.authenticated) return;
		void load();
		if (untrack(() => settings.current.heroStyle) !== 'off') {
			void api
				.featured()
				.then((r) => (featured = r.items))
				.catch(() => undefined);
		}
		// Recommendations may need TMDB; Home does not wait for them.
		// Read once: the effect must not rerun on every settings change.
		if (untrack(() => settings.current.homeRows.includes('because'))) {
			const ask = () =>
				void api
					.because()
					.then((r) => (because = r.rows))
					.catch(() => undefined);
			ask();
			// Another title of yours leads the row every two minutes, and a
			// Home left open follows it.
			const every = setInterval(ask, 120_000);
			return () => clearInterval(every);
		}
	});

	// Only those who show the row ask for everyone's watching, and that is
	// the account's own say: this browser's copy may be from before it changed.
	const showsActivity = $derived(settings.loaded && settings.current.homeRows.includes('activity'));
	$effect(() => {
		if (!session.authenticated || !showsActivity) return;
		void api
			.activity()
			.then((r) => (activity = r.items))
			.catch(() => undefined);
	});

	async function load() {
		loading = true;
		failed = null;
		try {
			data = await api.home();
		} catch (e) {
			failed = failure(e, { title: 'Could not load Home', hint: 'Try again in a minute.' });
		} finally {
			loading = false;
		}
	}

	/** What a right-click, or a hold, offers on a card; Home reads itself again after. */
	const menuOf = (item: LibraryItem) =>
		titleMenu(
			{ ...item, status: item.entry?.status ?? null, favourite: item.entry?.favourite },
			{ onchanged: load }
		);
	const menuOfNext = (entry: UpNextEntry) =>
		titleMenu(
			{
				...entry.item,
				status: entry.item.entry?.status ?? null,
				favourite: entry.item.entry?.favourite
			},
			{ progress: { next: entry.next, watched: entry.item.watched_episodes }, onchanged: load }
		);
	const menuOfFound = (item: SearchItem) =>
		titleMenu({ ...item, status: item.in_library }, { onchanged: load });

	const hero = $derived(data?.hero ?? null);
	const heroStyle = $derived(settings.current.heroStyle);
	/** Recommendations answered from the hero, out of it at once. */
	let answered = $state<number[]>([]);
	const slides = $derived(
		data
			? slidesOf(
					(data.recommended ?? []).filter((r) => !answered.includes(r.id)),
					featured,
					data.this_week,
					hero
				)
			: []
	);
	/** The hero turns through other things: its own title goes back in its row. */
	const rotating = $derived(slides.length > 0 && slides[0].kind !== 'today');
	/** Not shown as the hero, by rotation or by Settings: back in its own row. */
	const putBack = $derived(rotating || heroStyle === 'off');
	const heroItem = $derived(putBack ? null : (hero?.item ?? null));

	const isEmpty = $derived(
		!!data && !data.hero && !data.continue.length && !data.recent.length && !data.favourites.length
	);

	// Empty, and staying here: decided only once the account's own settings
	// say it was welcomed, so a new account never sees Home flash empty on its
	// way to the welcome.
	const showEmpty = $derived(isEmpty && settings.loaded && !!settings.current.onboarded);

	// A brand-new account starts with a welcome rather than an empty page.
	// It waits for the account's own settings, which load alongside Home, and
	// anyone who already tracks something never sees it.
	$effect(() => {
		if (isEmpty && settings.loaded && !settings.current.onboarded) {
			void goto('/welcome', { replace: true });
		}
	});

	/** What you are following, the hero's own title left out: in progress
	 *  first, then the shows waiting on their next episode. */
	const following = $derived.by(() => {
		if (!data) return [];
		const back = (row: 'continue' | 'waiting') =>
			putBack && hero && data?.hero_row === row ? [hero] : [];
		return [...back('continue'), ...data.continue, ...back('waiting'), ...data.waiting].filter(
			(e) => e.item.id !== heroItem?.id
		);
	});
	const recent = $derived(
		data
			? putBack && hero && data.hero_row === 'recent'
				? [hero.item, ...data.recent]
				: data.recent
			: []
	);

	const rows = $derived.by(() => {
		if (!data) return [];
		const available: Record<string, boolean> = {
			shelves: data.shelves.length > 0,
			continue: following.length > 0,
			thisweek: data.this_week.length > 0,
			recent: recent.length > 0,
			favourites: data.favourites.length > 0,
			because: because.length > 0,
			activity: activity.length > 0
		};
		return settings.current.homeRows.filter((id) => available[id]);
	});
</script>

<svelte:head>
	<title>Frog</title>
</svelte:head>

{#if loading && !data}
	<div class="page">
		<div class="hero-skeleton skeleton"></div>
		<RailSkeleton wide />
		<RailSkeleton />
	</div>
{:else if failed && !data}
	<div class="page"><LoadFailed {failed} onretry={load} /></div>
{:else if isEmpty}
	{#if showEmpty}
		<div class="page">
			<Empty
				art="hello"
				title="Nothing here yet"
				hint="Add a film or show and this page fills with what to watch next."
			>
				<a class="btn primary" href="/search">Find something</a>
				<a class="btn ghost" href="/import">Import a library</a>
			</Empty>
		</div>
	{/if}
{:else if data}
	<div class="home">
		{#if heroStyle !== 'off' && slides.length}
			<HomeHero
				{slides}
				style={heroStyle}
				onanswered={(id) => (answered = [...answered, id])}
				onadded={(id) => (featured = featured.filter((f) => f.item.id !== id))}
				onchanged={load}
			/>
		{/if}

		<div class="page rows">
			{#each rows as row (row)}
				{#if row === 'shelves'}
					{#snippet allShelves()}<a class="see-all hit" href="/shelves">All shelves</a>{/snippet}
					<Rail title="Your shelves" action={allShelves} width="216px">
						{#each data.shelves as shelf (shelf.id)}
							<ShelfCard {shelf} menu={shelfMenu(shelf, load)} />
						{/each}
						<a class="new-shelf" href="/shelves?new=1">
							<span class="plus"><Icon name="plus" size={22} /></span>
							<strong>New shelf</strong>
						</a>
					</Rail>
				{:else if row === 'continue'}
					<Rail title="Continue watching" wide>
						{#each following as entry (entry.item.id)}
							<ContinueCard {entry} menu={menuOfNext(entry)} />
						{/each}
					</Rail>
				{:else if row === 'activity'}
					{#snippet allActivity()}<a class="see-all hit" href="/activity">See all</a>{/snippet}
					<Rail title="Activity" action={allActivity} width="264px">
						{#each activity as item (`${item.who.username}:${item.title.id}`)}
							<ActivityCard {item} />
						{/each}
					</Rail>
				{:else if row === 'thisweek'}
					<section class="week">
						<div class="section-head">
							<h2>Airing this week</h2>
							<a class="see-all hit" href="/calendar">Calendar</a>
						</div>
						<ul>
							{#each data.this_week as airing (airing.item.id)}
								<li>
									<a href={titleHref(airing.item)}>
										{#if airing.item.poster_path}
											<img src={img(airing.item.poster_path, 'w92')} alt="" loading="lazy" />
										{:else}
											<span class="noart"></span>
										{/if}
										<span class="who">
											<span class="when"
												>{relativeDate(airing.air_date)} · {airTime(firstMoment(airing))}</span
											>
											<strong>{airing.item.name}</strong>
											<span class="faint">
												{episodeRange(airing)}
												{#if airing.episodes.length === 1 && airing.episodes[0].name}
													· {airing.episodes[0].name}
												{:else if airing.episodes.length > 1}
													· {airing.episodes.length} episodes
												{/if}
											</span>
										</span>
									</a>
								</li>
							{/each}
						</ul>
					</section>
				{:else if row === 'because'}
					<!-- One: a second is the same idea again. -->
					{#each because.slice(0, 1) as group (group.seed.id)}
						{#snippet viewMore()}<a class="see-all hit" href="/t/{group.seed.id}/more">View more</a
							>{/snippet}
						<Rail title="Because you watched {group.seed.name}" action={viewMore}>
							{#each uniqueBy(group.items, titleKey) as item (titleKey(item))}
								<Poster
									menu={menuOfFound(item)}
									kind={item.kind}
									ref={item}
									name={item.name}
									posterPath={item.poster_path}
									year={item.year}
								/>
							{/each}
						</Rail>
					{/each}
				{:else if row === 'recent'}
					<Rail title="Recently added">
						{#each recent as item (item.id)}
							<Poster
								menu={menuOf(item)}
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
					</Rail>
				{:else if row === 'favourites'}
					<Rail title="Favourites">
						{#each data.favourites as item (item.id)}
							<Poster
								menu={menuOf(item)}
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
					</Rail>
				{/if}
			{/each}
		</div>
	</div>
{/if}

<style>
	.see-all {
		color: var(--accent);
		font-size: 0.86rem;
		font-weight: 600;
		text-decoration: none;
	}

	.new-shelf {
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: var(--text-dim);
	}

	.new-shelf .plus {
		display: grid;
		place-items: center;
		aspect-ratio: 16 / 11;
		border-radius: var(--radius);
		border: 1.5px dashed var(--line-strong);
		transition:
			color var(--fast) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.new-shelf:hover .plus {
		color: var(--accent);
		border-color: var(--accent);
	}

	.new-shelf strong {
		font-size: 0.95rem;
		font-weight: 600;
		padding: 0 2px;
	}

	.hero-skeleton {
		height: min(52vh, 420px);
		margin: calc(var(--header-h) * -0.2) 0 28px;
		border-radius: var(--radius-lg);
	}

	/* Under the hero (HomeHero), the rows start close. */
	.home > :global(.hero + .rows) {
		padding-top: 8px;
	}

	.week {
		margin-bottom: 44px;
	}

	.week ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 300px), 1fr));
		gap: 10px;
	}

	.week li a {
		display: flex;
		align-items: center;
		gap: 14px;
		height: 100%;
		padding: 10px;
		border-radius: var(--radius);
		background: var(--bg-elev);
		transition: background var(--fast) var(--ease);
	}

	.week li a:hover {
		background: var(--bg-hover);
	}

	.week img,
	.week .noart {
		flex: none;
		width: 46px;
		aspect-ratio: 2 / 3;
		border-radius: 6px;
		object-fit: cover;
		background: var(--bg-elev-2);
		display: block;
	}

	.who {
		display: flex;
		flex-direction: column;
		min-width: 0;
		gap: 2px;
		flex: 1;
	}

	.who strong {
		font-size: 0.92rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.who .faint {
		font-size: 0.78rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.when {
		/* A block of its own, so its first letter can be a capital. */
		display: inline-block;
		font-size: 0.78rem;
		font-weight: 700;
		color: var(--accent);
	}

	.when::first-letter {
		text-transform: uppercase;
	}
</style>
