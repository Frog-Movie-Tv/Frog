<script lang="ts">
	import { api, img } from '#lib/api';
	import ContinueCard from '#lib/components/ContinueCard.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import RailSkeleton from '#lib/components/RailSkeleton.svelte';
	import { episodeCode, prettyDate, relativeDate, runtime, year } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { AiringEntry, HomeData } from '#lib/types';

	let data = $state<HomeData | null>(null);
	let loading = $state(true);

	$effect(() => {
		if (!session.authenticated) return;
		void load();
	});

	async function load() {
		try {
			data = await api.home();
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = false;
		}
	}

	const hero = $derived(data?.hero ?? null);
	const heroItem = $derived(hero?.item ?? null);
	const heroNext = $derived(hero?.next ?? null);
	const heroCaughtUp = $derived(!!heroNext && !heroNext.aired);
	const heroStyle = $derived(settings.current.heroStyle);

	const isEmpty = $derived(
		!!data && !data.hero && !data.continue.length && !data.recent.length && !data.favourites.length
	);

	const rows = $derived.by(() => {
		if (!data) return [];
		const available: Record<string, boolean> = {
			continue: data.continue.length > 0,
			upnext: data.waiting.length > 0,
			thisweek: data.this_week.length > 0,
			recent: data.recent.length > 0,
			favourites: data.favourites.length > 0
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
					<img class="backdrop" src={img(heroItem.backdrop_path, 'w1280')} alt="" fetchpriority="high" />
				{:else if heroStyle === 'poster' && heroItem.poster_path}
					<img class="backdrop poster" src={img(heroItem.poster_path, 'w780')} alt="" fetchpriority="high" />
				{/if}
				<div class="veil" aria-hidden="true"></div>

				<div class="hero-body">
					<span class="eyebrow">{heroCaughtUp ? 'Caught up' : 'Continue watching'}</span>

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
							<a class="btn" href="/title/{heroItem.kind}/{heroItem.tmdb_id}">View show</a>
						{:else}
							<a class="btn primary" href="/title/{heroItem.kind}/{heroItem.tmdb_id}">
								<Icon name="play" size={14} filled />
								{heroNext ? 'Next episode' : 'Open'}
							</a>
						{/if}
						<a class="btn ghost" href="/library">Library</a>
					</div>
				</div>
			</header>
		{/if}

		<div class="page rows">
			{#each rows as row (row)}
				{#if row === 'continue'}
					<Rail title="Continue watching" wide>
						{#each data.continue as entry (entry.item.id)}
							<ContinueCard {entry} />
						{/each}
					</Rail>
				{:else if row === 'upnext'}
					<Rail title="Caught up" wide>
						{#each data.waiting as entry (entry.item.id)}
							<ContinueCard {entry} />
						{/each}
					</Rail>
				{:else if row === 'thisweek'}
					<section class="week">
						<div class="section-head"><h2>Airing this week</h2></div>
						<ul>
							{#each data.this_week as airing (airing.item.id)}
								<li>
									<a href="/title/{airing.item.kind}/{airing.item.tmdb_id}">
										{#if airing.item.poster_path}
											<img src={img(airing.item.poster_path, 'w92')} alt="" loading="lazy" />
										{/if}
										<span class="who">
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
										<span class="when">{relativeDate(airing.air_date)}</span>
									</a>
								</li>
							{/each}
						</ul>
					</section>
				{:else if row === 'recent'}
					<Rail title="Recently added">
						{#each data.recent as item (item.id)}
							<Poster
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
					</Rail>
				{:else if row === 'favourites'}
					<Rail title="Favourites">
						{#each data.favourites as item (item.id)}
							<Poster
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
					</Rail>
				{/if}
			{/each}
		</div>
	</div>
{/if}

<style>
	.hero {
		position: relative;
		isolation: isolate;
		min-height: min(62vh, 520px);
		display: flex;
		align-items: flex-end;
		padding: calc(var(--header-h) + 40px) var(--page-pad) 34px;
		background: var(--tint, var(--bg-elev));
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
				var(--bg) 1%,
				color-mix(in srgb, var(--bg) 40%, transparent) 50%,
				transparent
			),
			linear-gradient(to right, color-mix(in srgb, var(--tint) 78%, transparent), transparent 65%);
	}

	.hero-body {
		max-width: 1500px;
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
		padding-top: 30px;
	}

	.week {
		margin-bottom: 40px;
	}

	.week ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.week a {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		transition: border-color var(--fast) var(--ease);
	}

	.week a:hover {
		border-color: var(--line-strong);
	}

	.week img {
		width: 34px;
		border-radius: 5px;
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
		font-size: 0.9rem;
		font-weight: 600;
	}

	.who .faint {
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.when {
		font-size: 0.8rem;
		color: var(--accent);
		font-weight: 600;
		flex: none;
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
