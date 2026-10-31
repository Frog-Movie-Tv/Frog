<script lang="ts">
	import { titleHref, titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import ContinueCard from '#lib/components/ContinueCard.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import RailSkeleton from '#lib/components/RailSkeleton.svelte';
	import ShelfCard from '#lib/components/ShelfCard.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { episodeCode, prettyDate, relativeDate, runtime, year } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import type { AiringEntry, BecauseRow, HomeData } from '#lib/types';

	let data = $state<HomeData | null>(null);
	let loading = $state(true);
	let failed = $state<Failure | null>(null);
	let because = $state<BecauseRow[]>([]);

	$effect(() => {
		if (!session.authenticated) return;
		void load();
		// Recommendations may need TMDB; Home does not wait for them.
		// Read once: the effect must not rerun on every settings change.
		if (untrack(() => settings.current.homeRows.includes('because'))) {
			void api
				.because()
				.then((r) => (because = r.rows))
				.catch(() => undefined);
		}
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

	const hero = $derived(data?.hero ?? null);
	const heroItem = $derived(hero?.item ?? null);
	const heroNext = $derived(hero?.next ?? null);
	const heroCaughtUp = $derived(!!heroNext && !heroNext.aired);
	/** Marked as watching, but not a single episode ticked yet. */
	const heroFresh = $derived(
		!!heroItem && heroItem.kind === 'tv' && heroItem.watched_episodes === 0 && !heroCaughtUp
	);
	const heroLabel = $derived(
		heroCaughtUp
			? 'Caught up'
			: heroFresh
				? 'Start watching'
				: heroNext || heroItem?.entry?.status === 'watching'
					? 'Continue watching'
					: // Nothing in progress: the hero is simply the newest addition.
						'Recently added'
	);
	const heroStyle = $derived(settings.current.heroStyle);

	const isEmpty = $derived(
		!!data && !data.hero && !data.continue.length && !data.recent.length && !data.favourites.length
	);

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
	const following = $derived(
		data ? [...data.continue, ...data.waiting].filter((e) => e.item.id !== heroItem?.id) : []
	);

	const rows = $derived.by(() => {
		if (!data) return [];
		const available: Record<string, boolean> = {
			shelves: data.shelves.length > 0,
			continue: following.length > 0,
			thisweek: data.this_week.length > 0,
			recent: data.recent.length > 0,
			favourites: data.favourites.length > 0,
			because: because.length > 0
		};
		return settings.current.homeRows.filter((id) => available[id]);
	});

	/** "S01E05" for one, "S01E05–E06" for a run of them. */
	function episodeRange(entry: AiringEntry): string {
		const first = entry.episodes[0];
		const last = entry.episodes[entry.episodes.length - 1];
		const start = episodeCode(first.season_number, first.episode_number);
		if (entry.episodes.length === 1) return start;
		const end =
			last.season_number === first.season_number
				? `E${String(last.episode_number).padStart(2, '0')}`
				: episodeCode(last.season_number, last.episode_number);
		return `${start}–${end}`;
	}
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
	<div class="page">
		<Empty
			art="search"
			title="Nothing here yet"
			hint="Add a film or show and this page fills with what to watch next."
		>
			<a class="btn primary" href="/search">Find something</a>
			<a class="btn ghost" href="/import">Import a library</a>
		</Empty>
	</div>
{:else if data}
	<div class="home">
		{#if heroItem && heroStyle !== 'off'}
			<header class="hero" style:--tint={heroItem.accent.shade}>
				{#if heroStyle === 'backdrop' && heroItem.backdrop_path}
					<img
						class="backdrop"
						src={img(heroItem.backdrop_path, 'w1280')}
						alt=""
						fetchpriority="high"
					/>
				{:else if heroStyle === 'poster' && heroItem.poster_path}
					<img
						class="backdrop poster"
						src={img(heroItem.poster_path, 'w780')}
						alt=""
						fetchpriority="high"
					/>
				{/if}
				<div class="veil" aria-hidden="true"></div>

				<div class="hero-body">
					<span class="hero-label">{heroLabel}</span>

					{#if heroItem.logo_path}
						<img class="logo" src={img(heroItem.logo_path, 'w500')} alt={heroItem.name} />
					{:else}
						<h1>{heroItem.name}</h1>
					{/if}

					<p class="facts">
						{#if year(heroItem.release_date)}<span>{year(heroItem.release_date)}</span>{/if}
						{#if heroItem.kind === 'movie' && heroItem.runtime}
							<span>{runtime(heroItem.runtime)}</span>
						{/if}
						{#if heroItem.genres.length}<span>{heroItem.genres.slice(0, 3).join(' · ')}</span>{/if}
					</p>

					{#if heroNext}
						<p class="upnext">
							{#if heroCaughtUp}
								Next episode {heroNext.air_date ? prettyDate(heroNext.air_date) : 'to be announced'}
							{:else}
								<strong>{episodeCode(heroNext.season_number, heroNext.episode_number)}</strong>
								{heroNext.name ?? ''}
							{/if}
						</p>
					{/if}

					<div class="hero-actions">
						{#if heroCaughtUp}
							<a class="btn" href={titleHref(heroItem)}>View show</a>
						{:else}
							<a class="btn primary" href={titleHref(heroItem)}>
								<Icon name="play" size={14} filled />
								{heroFresh ? 'Start watching' : heroNext ? 'Next episode' : 'Open'}
							</a>
						{/if}
					</div>
				</div>
			</header>
		{/if}

		<div class="page rows">
			{#each rows as row (row)}
				{#if row === 'shelves'}
					{#snippet allShelves()}<a class="see-all hit" href="/shelves">All shelves</a>{/snippet}
					<Rail title="Your shelves" action={allShelves} width="216px">
						{#each data.shelves as shelf (shelf.id)}
							<ShelfCard {shelf} />
						{/each}
						<a class="new-shelf" href="/shelves?new=1">
							<span class="plus"><Icon name="plus" size={22} /></span>
							<strong>New shelf</strong>
						</a>
					</Rail>
				{:else if row === 'continue'}
					<Rail title="Continue watching" wide>
						{#each following as entry (entry.item.id)}
							<ContinueCard {entry} />
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
											<span class="when">{relativeDate(airing.air_date)}</span>
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
						<Rail title="Because you watched {group.seed.name}">
							{#each uniqueBy(group.items, titleKey) as item (titleKey(item))}
								<Poster
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
						{#each data.recent as item (item.id)}
							<Poster
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

	.hero {
		position: relative;
		isolation: isolate;
		min-height: min(56vh, 480px);
		display: flex;
		align-items: flex-end;
		padding: calc(var(--header-h) + 48px) var(--page-pad) 36px;
		background: linear-gradient(to bottom, var(--tint, var(--bg-elev)) 40%, var(--bg));
		overflow: hidden;
	}

	.hero-skeleton {
		height: min(52vh, 420px);
		margin: calc(var(--header-h) * -0.2) 0 28px;
		border-radius: var(--radius-lg);
	}

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
		z-index: -2;
		/* The picture fades out itself, so its edge never shows as a line where
		   the page begins. */
		mask-image: linear-gradient(to top, transparent, #000 30%);
	}

	.backdrop.poster {
		object-position: center 12%;
	}

	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(
				to top,
				var(--bg) 0%,
				color-mix(in srgb, var(--bg) 82%, transparent) 26%,
				color-mix(in srgb, var(--bg) 30%, transparent) 62%,
				transparent 100%
			),
			linear-gradient(
				to right,
				color-mix(in srgb, var(--bg) 94%, transparent) 0%,
				color-mix(in srgb, var(--bg) 72%, transparent) 38%,
				transparent 72%
			);
	}

	/* Accent and bold: the old faint caption was close to invisible over
	   artwork, and outright invisible on Pond Light. */
	.hero-label {
		font-size: 0.74rem;
		font-weight: 750;
		letter-spacing: 0.11em;
		text-transform: uppercase;
		color: var(--accent);
	}

	.hero .facts,
	.hero .upnext {
		color: var(--text);
	}

	.hero-body {
		max-width: 1440px;
		margin: 0 auto;
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
	}

	.logo {
		max-width: min(460px, 76vw);
		max-height: 140px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.65));
	}

	/* On a dark theme a black logo would vanish into the page; a faint light
	   halo keeps its outline without changing a coloured one. */
	:global(:root:not([data-theme='light']):not([data-theme='pond-light'])) .logo {
		filter: drop-shadow(0 0 1px rgb(255 255 255 / 0.45))
			drop-shadow(0 0 14px rgb(255 255 255 / 0.12)) drop-shadow(0 4px 18px rgb(0 0 0 / 0.5));
	}

	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
		font-size: 0.88rem;
		color: var(--text-dim);
	}

	.upnext {
		font-size: 0.95rem;
		color: var(--text-dim);
	}

	.upnext strong {
		color: var(--accent);
		font-variant-numeric: tabular-nums;
		margin-right: 6px;
	}

	.hero-actions {
		display: flex;
		gap: 10px;
		margin-top: 6px;
	}

	.rows {
		padding-top: 32px;
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
		background: var(--bg-elev-2);
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
		font-size: 0.72rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--accent);
	}

	@media (max-width: 720px) {
		.hero {
			min-height: 48vh;
			padding-top: calc(var(--header-h) + 24px);
		}

		.logo {
			max-height: 96px;
		}
	}
</style>
