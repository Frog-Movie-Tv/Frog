<script lang="ts">
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Heatmap from '#lib/components/Heatmap.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import SeasonList from '#lib/components/SeasonList.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import TrackControls from '#lib/components/TrackControls.svelte';
	import { episodeCode, prettyDate, runtime, year } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { whenWatched } from '#lib/when.svelte';
	import type { Kind, LibraryItem, TitleDetail } from '#lib/types';

	let detail = $state<TitleDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let trailerOpen = $state<string | null>(null);

	const kind = $derived(page.params.kind as Kind);
	const tmdbId = $derived(Number(page.params.id));

	$effect(() => {
		void load(kind, tmdbId);
	});

	let token = 0;

	async function load(k: Kind, id: number) {
		const mine = ++token;
		loading = true;
		error = null;
		try {
			const res = await api.title(k, id);
			if (mine !== token) return;
			detail = res;
		} catch (e) {
			if (mine !== token) return;
			error = e instanceof Error ? e.message : 'Could not load this title';
			detail = null;
		} finally {
			if (mine === token) loading = false;
		}
	}

	function updateItem(item: LibraryItem) {
		if (detail) detail = { ...detail, item };
	}

	async function refreshAfterEpisodes(item: LibraryItem) {
		updateItem(item);
		// Pull the heatmap and "up next" again, cheaply, after a tick change.
		try {
			const fresh = await api.title(kind, tmdbId);
			detail = fresh;
		} catch {
			// The optimistic update already landed; a stale heatmap can wait.
		}
	}

	const item = $derived(detail?.item);
	const accent = $derived(
		settings.current.followPoster && item ? item.accent : null
	);

	const metaLine = $derived.by(() => {
		if (!item) return [];
		const bits: string[] = [];
		const y = year(item.release_date);
		if (y) bits.push(y);
		if (item.kind === 'movie' && item.runtime) bits.push(runtime(item.runtime));
		if (item.kind === 'tv' && item.season_count) {
			bits.push(`${item.season_count} season${item.season_count === 1 ? '' : 's'}`);
		}
		if (item.kind === 'tv' && item.episode_count) bits.push(`${item.episode_count} episodes`);
		if (item.tmdb_status && item.kind === 'tv') bits.push(item.tmdb_status);
		return bits;
	});

	const providers = $derived(detail?.providers);
	const providerGroups = $derived(
		providers
			? [
					{ label: 'Stream', list: providers.flatrate },
					{ label: 'Rent', list: providers.rent },
					{ label: 'Buy', list: providers.buy }
				].filter((g) => g.list.length)
			: []
	);
	const hasProviders = $derived(providerGroups.length > 0);

	let markingAll = $state(false);
	/** Bumped to remount the season list after a whole-show change. */
	let seasonsVersion = $state(0);

	async function markAllWatched() {
		if (!item) return;
		const answer = await whenWatched.ask(item.name, 'Every aired episode');
		if (!answer) return;
		markingAll = true;
		try {
			const { item: updated } = await api.setShowWatched(kind, tmdbId, true, answer);
			await refreshAfterEpisodes(updated);
			seasonsVersion++;
			toasts.push(`Marked ${updated.watched_episodes} episodes watched`);
		} catch (e) {
			toasts.error(e);
		} finally {
			markingAll = false;
		}
	}

	async function markFilmWatched() {
		if (!item) return;
		try {
			const { item: updated } = await api.track(kind, tmdbId, { status: 'completed' });
			updateItem(updated);
			toasts.push('Marked as watched');
		} catch (e) {
			toasts.error(e);
		}
	}
</script>

<svelte:head>
	<title>{item ? `${item.name} · Frog` : 'Frog'}</title>
</svelte:head>

{#if loading && !detail}
	<div class="page"><Spinner label="Loading" /></div>
{:else if error}
	<div class="page"><Empty title="Could not load this title" hint={error} /></div>
{:else if detail && item}
	<article
		class="title-page"
		style:--accent={accent?.accent}
		style:--accent-soft={accent ? `color-mix(in srgb, ${accent.accent} 18%, transparent)` : undefined}
		style:--page-shade={accent?.shade ?? 'var(--bg)'}
	>
		<header class="hero">
			{#if item.backdrop_path}
				<img class="backdrop" src={img(item.backdrop_path, 'w1280')} alt="" fetchpriority="high" />
			{/if}
			<div class="veil" aria-hidden="true"></div>

			<div class="hero-inner">
				<div class="poster-slot" style:view-transition-name="poster-{kind}-{tmdbId}">
					{#if item.poster_path}
						<img src={img(item.poster_path, 'w342')} alt="Poster for {item.name}" />
					{/if}
				</div>

				<div class="hero-text">
					{#if item.logo_path}
						<img class="logo" src={img(item.logo_path, 'w500')} alt={item.name} />
					{:else}
						<h1>{item.name}</h1>
					{/if}

					{#if detail.tagline}
						<p class="tagline">{detail.tagline}</p>
					{/if}

					<div class="meta">
						{#each metaLine as bit, i (bit)}
							{#if i > 0}<span class="dot" aria-hidden="true">•</span>{/if}
							<span>{bit}</span>
						{/each}
						{#if item.tmdb_vote}
							<span class="dot" aria-hidden="true">•</span>
							<span class="tmdb">TMDB {item.tmdb_vote.toFixed(1)}</span>
						{/if}
					</div>

					{#if item.genres.length}
						<div class="genres">
							{#each item.genres as g (g)}<span class="pill">{g}</span>{/each}
						</div>
					{/if}

					<div class="actions">
						<TrackControls {item} onchange={updateItem} />
						{#if item.kind === 'movie' && item.entry && item.entry.status !== 'completed'}
							<button class="btn" onclick={markFilmWatched}>Mark watched</button>
						{/if}
					</div>

					{#if detail.next_up && item.entry}
						<a class="next" href="#seasons">
							<span class="eyebrow">Up next</span>
							<strong>
								{episodeCode(detail.next_up.season_number, detail.next_up.episode_number)}
								{detail.next_up.name ?? ''}
							</strong>
							{#if detail.next_up.air_date}
								<span class="faint">{prettyDate(detail.next_up.air_date)}</span>
							{/if}
						</a>
					{/if}

					{#if item.total_episodes > 0}
						<div class="progress-line">
							<div class="bar">
								<i style:width="{(item.watched_episodes / item.total_episodes) * 100}%"></i>
							</div>
							<span class="faint">
								{item.watched_episodes} / {item.total_episodes} episodes
							</span>
						</div>
					{/if}
				</div>
			</div>
		</header>

		<div class="page body">
			{#if item.overview}
				<section class="block">
					<p class="overview">{item.overview}</p>
				</section>
			{/if}

			{#if hasProviders && providers}
				<section class="block">
					<div class="section-head">
						<h2>Where to watch</h2>
						<span class="faint">{detail.provider_region}</span>
					</div>
					<div class="providers">
						{#each providerGroups as group (group.label)}
							{@const label = group.label}
							{@const list = group.list}
							{#if list.length}
								<div class="pgroup">
									<span class="eyebrow">{label}</span>
									<div class="logos">
										{#each list as p (p.id)}
											<!-- TMDB gives one watch link per region, not per provider, so
											     every logo opens the same official page for this title. -->
											<a
												class="plogo"
												href={providers?.link ?? `https://www.themoviedb.org/search?query=${encodeURIComponent(item.name)}`}
												target="_blank"
												rel="noreferrer noopener"
												title="{p.name} — open watch options"
											>
												{#if p.logo_path}
													<img src={img(p.logo_path, 'w92')} alt={p.name} loading="lazy" />
												{:else}
													{p.name}
												{/if}
											</a>
										{/each}
									</div>
								</div>
							{/if}
						{/each}
						{#if providers.link}
							<a class="btn ghost" href={providers.link} target="_blank" rel="noreferrer noopener">
								Open on TMDB
							</a>
						{/if}
					</div>
				</section>
			{/if}

			{#if item.kind === 'tv' && detail.seasons.length}
				<section class="block" id="seasons">
					<div class="section-head">
						<h2>Seasons</h2>
						<button class="btn ghost small" disabled={markingAll} onclick={markAllWatched}>
							<Icon name="check" size={14} />
							Mark all watched
						</button>
					</div>
					{#key seasonsVersion}
						<SeasonList
							{kind}
							{tmdbId}
							showName={item.name}
							seasons={detail.seasons}
							initialSeason={detail.next_up?.season_number ?? null}
							onprogress={refreshAfterEpisodes}
						/>
					{/key}
				</section>

				{#if detail.heatmap.length}
					<section class="block">
						<div class="section-head"><h2>Episode ratings</h2></div>
						<Heatmap cells={detail.heatmap} />
					</section>
				{/if}
			{/if}

			{#if detail.cast.length}
				<section class="block">
					<Rail title="Cast">
						{#each detail.cast as person (person.id)}
							<a class="person" href="/person/{person.id}">
								<div class="face">
									{#if person.profile_path}
										<img src={img(person.profile_path, 'w185')} alt="" loading="lazy" />
									{:else}
										<span aria-hidden="true">{person.name.slice(0, 1)}</span>
									{/if}
								</div>
								<span class="pname">{person.name}</span>
								{#if person.character}<span class="role faint">{person.character}</span>{/if}
							</a>
						{/each}
					</Rail>
				</section>
			{/if}

			{#if detail.crew.length}
				<section class="block crew">
					<div class="section-head"><h2>Crew</h2></div>
					<div class="crew-list">
						{#each detail.crew as c (`${c.id}-${c.job}`)}
							<a href="/person/{c.id}" class="crew-item">
								<strong>{c.name}</strong>
								<span class="faint">{c.job}</span>
							</a>
						{/each}
					</div>
				</section>
			{/if}

			{#if detail.trailers.length}
				<section class="block">
					<div class="section-head"><h2>Trailers</h2></div>
					<div class="trailers">
						{#each detail.trailers as t (t.key)}
							{#if trailerOpen === t.key}
								<div class="player">
									<iframe
										src="https://www.youtube-nocookie.com/embed/{t.key}?autoplay=1"
										title={t.name}
										allow="accelerometer; autoplay; clipboard-write; encrypted-media; picture-in-picture"
										allowfullscreen
									></iframe>
								</div>
							{:else}
								<button class="trailer" onclick={() => (trailerOpen = t.key)}>
									<span class="sheen" aria-hidden="true"></span>
									<span class="play" aria-hidden="true">
										<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
											<path d="M8 5.14v13.72a1 1 0 0 0 1.5.86l11-6.86a1 1 0 0 0 0-1.72l-11-6.86A1 1 0 0 0 8 5.14z" />
										</svg>
									</span>
									<span class="tname">{t.name}</span>
									<span class="tkind">{t.kind}</span>
								</button>
							{/if}
						{/each}
					</div>
					<p class="faint tiny">Playing a trailer loads it from YouTube.</p>
				</section>
			{/if}

			{#if detail.collection?.parts?.length}
				<section class="block">
					<Rail title={detail.collection.name ?? 'Franchise'}>
						{#each detail.collection.parts as part (part.tmdb_id)}
							<Poster
								kind={part.kind}
								tmdbId={part.tmdb_id}
								name={part.name}
								posterPath={part.poster_path}
								year={part.year}
								status={part.in_library}
							/>
						{/each}
					</Rail>
					<p class="faint tiny">Franchise shown in release order.</p>
				</section>
			{/if}

			{#if detail.recommendations.length}
				<section class="block">
					<Rail title="More like this">
						{#each detail.recommendations as rec (`${rec.kind}-${rec.tmdb_id}`)}
							<Poster
								kind={rec.kind}
								tmdbId={rec.tmdb_id}
								name={rec.name}
								posterPath={rec.poster_path}
								year={rec.year}
								status={rec.in_library}
							/>
						{/each}
					</Rail>
				</section>
			{/if}
		</div>
	</article>
{/if}

<style>
	.title-page {
		/* The poster's dark companion bleeds behind the whole page, which is
		   what makes two different films feel like two different pages. */
		background:
			linear-gradient(var(--page-shade), transparent 680px),
			var(--bg);
		min-height: 100dvh;
	}

	.hero {
		position: relative;
		isolation: isolate;
		padding: calc(var(--header-h) + 40px) var(--page-pad) 30px;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 18%;
		z-index: -2;
	}

	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(to top, var(--bg) 2%, color-mix(in srgb, var(--bg) 55%, transparent) 45%, transparent),
			linear-gradient(to right, color-mix(in srgb, var(--page-shade) 85%, transparent), transparent 70%);
	}

	.hero-inner {
		max-width: 1500px;
		margin: 0 auto;
		display: flex;
		gap: 30px;
		align-items: flex-end;
		min-height: 330px;
	}

	.poster-slot {
		flex: none;
		width: 190px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius);
		overflow: hidden;
		box-shadow: var(--shadow-3);
		background: var(--bg-elev-2);
	}

	.poster-slot img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.hero-text {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-width: 0;
		padding-bottom: 6px;
	}

	.logo {
		max-width: min(420px, 70vw);
		max-height: 130px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.6));
	}

	.tagline {
		font-style: italic;
		color: var(--text-dim);
		font-size: 0.95rem;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		font-size: 0.88rem;
		color: var(--text-dim);
	}

	.dot {
		opacity: 0.5;
	}

	.tmdb {
		font-weight: 600;
		color: var(--text);
	}

	.genres {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-top: 4px;
	}

	.next {
		display: inline-flex;
		align-items: baseline;
		gap: 10px;
		padding: 10px 16px;
		border-radius: var(--radius-sm);
		background: var(--accent-soft);
		border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		width: fit-content;
		max-width: 100%;
	}

	.next strong {
		font-weight: 600;
		font-size: 0.92rem;
	}

	.progress-line {
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 0.8rem;
	}

	.bar {
		width: min(280px, 50vw);
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width var(--slow) var(--ease);
	}

	.body {
		padding-top: 28px;
	}

	.block {
		margin-bottom: 44px;
	}

	.btn.small {
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.overview {
		max-width: 76ch;
		font-size: 1rem;
		color: var(--text-dim);
	}

	.providers {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 22px;
	}

	.pgroup {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.logos {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	.plogo {
		display: inline-flex;
		border-radius: 11px;
		transition:
			transform var(--fast) var(--ease),
			box-shadow var(--fast) var(--ease);
	}

	.plogo:hover {
		transform: translateY(-2px);
		box-shadow: var(--shadow-2);
	}

	.plogo img {
		width: 44px;
		height: 44px;
		border-radius: 11px;
		object-fit: cover;
		border: 1px solid var(--line);
	}

	.person {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.face {
		aspect-ratio: 1;
		border-radius: 50%;
		overflow: hidden;
		background: var(--bg-elev-2);
		display: grid;
		place-items: center;
		font-size: 1.3rem;
		color: var(--text-faint);
		transition: transform var(--slow) var(--ease);
	}

	.person:hover .face {
		transform: scale(1.04);
	}

	.face img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.pname {
		font-size: 0.82rem;
		font-weight: 550;
		line-height: 1.25;
	}

	.role {
		font-size: 0.73rem;
		line-height: 1.25;
	}

	.crew-list {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 10px;
	}

	.crew-item {
		display: flex;
		flex-direction: column;
		padding: 10px 14px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		border: 1px solid var(--line);
		font-size: 0.86rem;
	}

	.crew-item:hover {
		border-color: var(--line-strong);
	}

	.crew-item span {
		font-size: 0.75rem;
	}

	.trailers {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 14px;
	}

	.trailer {
		position: relative;
		padding: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		cursor: pointer;
		aspect-ratio: 16 / 9;
	}

	/* Drawn locally rather than pulled from YouTube: a removed video used to
	   404 in the console, and nothing should reach a third party before you
	   press play. */
	.sheen {
		position: absolute;
		inset: 0;
		background:
			radial-gradient(
				120% 100% at 50% 0%,
				color-mix(in srgb, var(--accent) 22%, transparent),
				transparent 70%
			),
			var(--bg-elev-2);
	}

	.play {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--text);
		transition: transform var(--fast) var(--ease);
	}

	.trailer:hover .play {
		transform: scale(1.12);
		color: var(--accent);
	}

	.tkind {
		position: absolute;
		top: 8px;
		left: 10px;
		font-size: 0.66rem;
		font-weight: 650;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-faint);
	}

	.tname {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		padding: 18px 12px 9px;
		font-size: 0.78rem;
		text-align: left;
		color: var(--text);
		background: linear-gradient(transparent, color-mix(in srgb, var(--bg) 85%, transparent));
	}

	.player {
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		border: 1px solid var(--line-strong);
	}

	.player iframe {
		width: 100%;
		height: 100%;
		border: none;
	}

	.tiny {
		font-size: 0.72rem;
		margin-top: 10px;
	}

	@media (max-width: 860px) {
		.hero-inner {
			flex-direction: column;
			align-items: flex-start;
			gap: 18px;
			min-height: 0;
		}

		.poster-slot {
			width: 124px;
		}

		.hero {
			padding-top: calc(var(--header-h) + 24px);
		}
	}
</style>
