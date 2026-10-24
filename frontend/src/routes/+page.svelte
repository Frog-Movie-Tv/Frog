<script lang="ts">
	import { api, img } from '#lib/api';
	import ContinueCard from '#lib/components/ContinueCard.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { episodeCode, prettyDate, relativeDate, runtime, year } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { HomeData } from '#lib/types';

	let data = $state<HomeData | null>(null);
	let loading = $state(true);

	$effect(() => {
		if (!session.authenticated) return;
		void api
			.home()
			.then((d) => (data = d))
			.catch((e) => toasts.error(e))
			.finally(() => (loading = false));
	});

	const hero = $derived(data?.hero ?? null);
	const heroNext = $derived(data?.continue.find((c) => c.item.id === hero?.id)?.next ?? null);
	const heroStyle = $derived(settings.current.heroStyle);

	const isEmpty = $derived(
		!!data &&
			!data.hero &&
			!data.continue.length &&
			!data.recent.length &&
			!data.favourites.length
	);

	/** Rows in the order chosen in Settings, skipping any that have nothing. */
	const rows = $derived.by(() => {
		if (!data) return [];
		const available: Record<string, boolean> = {
			continue: data.continue.length > 0,
			upnext: data.continue.some((c) => c.next?.aired),
			thisweek: data.this_week.length > 0,
			recent: data.recent.length > 0,
			favourites: data.favourites.length > 0
		};
		return settings.current.homeRows.filter((id) => available[id]);
	});
</script>

<svelte:head>
	<title>Frog</title>
</svelte:head>

{#if loading && !data}
	<div class="page"><Spinner label="Warming up" /></div>
{:else if isEmpty}
	<div class="page">
		<Empty
			title="Your pond is empty"
			hint="Find a film or show and add it. Once something is in your library, this page fills with what to watch next."
		>
			<a class="btn primary" href="/search">Find something to watch</a>
		</Empty>
	</div>
{:else if data}
	<div class="home">
		{#if hero && heroStyle !== 'off'}
			<header
				class="hero"
				class:poster-mode={heroStyle === 'poster'}
				style:--tint={hero.accent.shade}
			>
				{#if heroStyle === 'backdrop' && hero.backdrop_path}
					<img class="backdrop" src={img(hero.backdrop_path, 'w1280')} alt="" fetchpriority="high" />
				{:else if heroStyle === 'poster' && hero.poster_path}
					<img class="backdrop poster" src={img(hero.poster_path, 'w780')} alt="" fetchpriority="high" />
				{/if}
				<div class="veil" aria-hidden="true"></div>

				<div class="hero-body">
					<span class="eyebrow">Continue watching</span>

					{#if hero.logo_path}
						<img class="logo" src={img(hero.logo_path, 'w500')} alt={hero.name} />
					{:else}
						<h1>{hero.name}</h1>
					{/if}

					<p class="facts">
						{#if year(hero.release_date)}<span>{year(hero.release_date)}</span>{/if}
						{#if hero.kind === 'movie' && hero.runtime}<span>{runtime(hero.runtime)}</span>{/if}
						{#if hero.genres.length}<span>{hero.genres.slice(0, 3).join(' · ')}</span>{/if}
					</p>

					{#if heroNext}
						<p class="upnext">
							<strong>{episodeCode(heroNext.season_number, heroNext.episode_number)}</strong>
							{heroNext.name ?? ''}
							{#if !heroNext.aired && heroNext.air_date}
								<span class="faint">· airs {prettyDate(heroNext.air_date)}</span>
							{/if}
						</p>
					{/if}

					<div class="hero-actions">
						<a class="btn primary" href="/title/{hero.kind}/{hero.tmdb_id}">Open</a>
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
				{:else if row === 'thisweek'}
					<section class="week">
						<div class="section-head"><h2>Airing this week</h2></div>
						<ul>
							{#each data.this_week as airing (`${airing.item.id}-${airing.episode.season_number}-${airing.episode.episode_number}`)}
								<li>
									<a href="/title/{airing.item.kind}/{airing.item.tmdb_id}">
										{#if airing.item.poster_path}
											<img src={img(airing.item.poster_path, 'w92')} alt="" loading="lazy" />
										{/if}
										<span class="who">
											<strong>{airing.item.name}</strong>
											<span class="faint">
												{episodeCode(airing.episode.season_number, airing.episode.episode_number)}
												{#if airing.episode.name}· {airing.episode.name}{/if}
											</span>
										</span>
										<span class="when">{relativeDate(airing.episode.air_date)}</span>
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
			linear-gradient(to top, var(--bg) 1%, color-mix(in srgb, var(--bg) 40%, transparent) 50%, transparent),
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

	.week {
		margin-bottom: 40px;
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
