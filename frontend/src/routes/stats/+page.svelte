<script lang="ts">
	import Rail from '#lib/components/Rail.svelte';
	import Score from '#lib/components/Score.svelte';
	import { titleHref } from '#lib/titles';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Select from '#lib/components/Select.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import YearInReview from '#lib/components/YearInReview.svelte';
	import { session } from '#lib/session.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { formatScore, longRuntime, plural, runtime } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { FullStats, MangaStats, StatsPerson, StatsTitle, Status } from '#lib/types';
	import { STATUSES, statusLabel } from '#lib/types';

	/*
	 * Two halves, as AniList keeps its own: films and shows, from what was
	 * watched, and manga, from what was read. Each has its own years.
	 */
	type Half = 'watching' | 'manga';

	const yearParam = $derived(Number(page.url.searchParams.get('year')) || null);
	const half = $derived<Half>(page.url.searchParams.get('half') === 'manga' ? 'manga' : 'watching');

	let stats = $state<FullStats | null>(null);
	let manga = $state<MangaStats | null>(null);
	let mangaFailed = $state<Failure | null>(null);
	let loading = $state(true);
	let failed = $state<Failure | null>(null);
	/** Pressing Try again asks for the same year again. */
	let tries = $state(0);
	let reviewYear = $state<number | null>(null);

	$effect(() => {
		if (!session.authenticated) return;
		const year = yearParam;
		void tries;
		let stale = false;
		loading = true;
		failed = null;
		void api
			.fullStats(year ?? undefined)
			.then((s) => {
				if (!stale) stats = s;
			})
			.catch((e) => {
				if (stale) return;
				failed = failure(e, { title: 'Could not load stats', hint: 'Try again in a minute.' });
			})
			.finally(() => {
				if (!stale) loading = false;
			});
		return () => (stale = true);
	});

	$effect(() => {
		if (!session.authenticated || half !== 'manga') return;
		const year = yearParam;
		void tries;
		let stale = false;
		mangaFailed = null;
		void api
			.mangaStats(year ?? undefined)
			.then((m) => {
				if (!stale) manga = m;
			})
			.catch((e) => {
				if (stale) return;
				mangaFailed = failure(e, { title: 'Could not load stats', hint: 'Try again in a minute.' });
			});
		return () => (stale = true);
	});

	/** The library as it stands, films and shows. */
	const library = $derived(stats?.library ?? null);
	/** Manga get a half of their own once there are any. */
	const hasManga = $derived((stats?.manga ?? 0) > 0 || half === 'manga');

	const yearOptions = $derived([
		{ value: 'all', label: 'All time' },
		...((half === 'manga' ? manga?.years : stats?.years) ?? []).map((y) => ({
			value: String(y),
			label: String(y)
		}))
	]);
	const shownYear = $derived(half === 'manga' ? (manga?.year ?? null) : (stats?.year ?? null));

	function address(nextHalf: Half, year: string | null): string {
		const parts: string[] = [];
		if (nextHalf === 'manga') parts.push('half=manga');
		if (year && year !== 'all') parts.push(`year=${encodeURIComponent(year)}`);
		return parts.length ? `/stats?${parts.join('&')}` : '/stats';
	}

	function pickYear(value: string) {
		void goto(address(half, value), { replace: true, reset: false });
	}

	function pickHalf(next: Half) {
		// Each half has its own years: a year one has, the other may not.
		void goto(address(next, null), { replace: true, reset: false });
	}

	const chaptersMax = $derived(Math.max(1, ...(manga?.timeline ?? []).map((t) => t.chapters)));
	const mangaGenreMax = $derived(Math.max(1, ...(manga?.genres ?? []).map((g) => g.titles)));
	const mangaRatingMax = $derived(Math.max(1, ...(manga?.ratings ?? [])));
	const mangaStatusTotal = $derived(
		STATUSES.reduce((sum, s) => sum + (manga?.by_status[s as Status] ?? 0), 0)
	);

	/** The year the review button makes: the one in view, else the latest. */
	const reviewTarget = $derived(stats?.year ?? stats?.years[0] ?? null);

	const statusTotal = $derived(STATUSES.reduce((sum, s) => sum + (library?.[s as Status] ?? 0), 0));

	const MONTHS = Array.from({ length: 12 }, (_, i) =>
		new Date(2000, i, 1).toLocaleDateString(undefined, { month: 'short' })
	);

	function unitLabel(unit: string): string {
		return unit.length === 7 ? (MONTHS[Number(unit.slice(5)) - 1] ?? unit) : unit;
	}

	const timelineMax = $derived(Math.max(1, ...(stats?.timeline ?? []).map((t) => t.minutes)));
	const genreMax = $derived(Math.max(1, ...(stats?.genres ?? []).map((g) => g.titles)));
	const decadeMax = $derived(Math.max(1, ...(stats?.decades ?? []).map((d) => d.titles)));
	const ratingMax = $derived(Math.max(1, ...(stats?.ratings ?? [])));
	const rated = $derived((stats?.ratings ?? []).reduce((a, b) => a + b, 0));

	function initials(name: string): string {
		return name
			.split(/\s+/)
			.slice(0, 2)
			.map((w) => w[0] ?? '')
			.join('')
			.toUpperCase();
	}

	function shortDate(iso: string | null): string {
		if (!iso) return '';
		return new Date(`${iso}T12:00:00`).toLocaleDateString(undefined, {
			day: 'numeric',
			month: 'short'
		});
	}
</script>

<svelte:head>
	<title>Stats · Frog</title>
</svelte:head>

{#snippet people(title: string, list: StatsPerson[], base: string, logo = false)}
	{#if list.length}
		<section class="block card panel">
			<h2>{title}</h2>
			<ol class="people">
				{#each list as person, i (person.tmdb_id)}
					{@const src = img(person.image, 'w185')}
					<li>
						<a class="person" href="{base}/{person.tmdb_id}">
							<span class="rank">{i + 1}</span>
							<span class="avatar" class:logo>
								{#if src}
									<img {src} alt="" loading="lazy" />
								{:else}
									{initials(person.name)}
								{/if}
							</span>
							<span class="name">{person.name}</span>
							<span class="faint count">{plural(person.titles, 'title')}</span>
						</a>
					</li>
				{/each}
			</ol>
		</section>
	{/if}
{/snippet}

{#snippet titles(title: string, list: StatsTitle[], show: boolean)}
	{#if list.length}
		<!-- A rail, with its arrows: the list runs on past the screen, and says so. -->
		<section class="block">
			<Rail {title} width="112px">
				{#each list as t, i (t.id)}
					{@const src = img(t.poster_path, 'w185')}
					<a href={titleHref(t)} class="top">
						<span class="cover">
							{#if src}<img {src} alt="" loading="lazy" />{/if}
							<span class="badge">{i + 1}</span>
						</span>
						<span class="top-name">{t.name}</span>
						<span class="faint top-meta">
							{#if show}
								{plural(t.watches, 'episode')} · {longRuntime(t.minutes)}
							{:else if t.rating !== null}
								<Score value={t.rating} />
								{#if t.watches > 1}· {t.watches}×{/if}
							{:else}
								{t.watches > 1 ? `${t.watches}×` : runtime(t.minutes)}
							{/if}
						</span>
					</a>
				{/each}
			</Rail>
		</section>
	{/if}
{/snippet}

{#snippet mangaHalf()}
	{#if mangaFailed}
		<LoadFailed failed={mangaFailed} onretry={() => tries++} />
	{:else if !manga}
		<Spinner label="Counting" />
	{:else if manga.totals.manga === 0}
		<Empty
			title={manga.year ? `Nothing read in ${manga.year}` : 'No manga yet'}
			hint="Count the chapters you read and they show up here."
		>
			<a class="btn primary" href="/search">Find a manga</a>
		</Empty>
	{:else}
		<div class="body">
			<div class="tiles">
				<div class="tile card hero">
					<span class="eyebrow">Chapters read</span>
					<strong>{manga.totals.chapters.toLocaleString()}</strong>
					<span class="faint">{plural(manga.totals.manga, 'manga', 'manga')}</span>
				</div>
				<div class="tile card">
					<span class="eyebrow">Completed</span>
					<strong>{manga.totals.completed.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Days reading</span>
					<strong>{manga.totals.days.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Mean rating</span>
					<strong><Score value={manga.totals.mean_rating} /></strong>
					{#if manga.totals.rated}<span class="faint">{plural(manga.totals.rated, 'rating')}</span
						>{/if}
				</div>
			</div>

			{#if manga.timeline.some((t) => t.chapters > 0)}
				<section class="block card panel">
					<h2>{manga.year ? 'Chapters by month' : 'Chapters by year'}</h2>
					<div
						class="columns"
						class:wide={!manga.year}
						role="img"
						aria-label="Chapters read over time"
					>
						{#each manga.timeline as point (point.unit)}
							<div
								class="column"
								title="{unitLabel(point.unit)}: {plural(point.chapters, 'chapter')}"
							>
								<span class="value faint">{point.chapters || ''}</span>
								<span class="bar-v" style:height="{(point.chapters / chaptersMax) * 100}%"></span>
								<span class="label faint">{unitLabel(point.unit)}</span>
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if manga.top.length}
				<section class="block">
					<Rail title="Top manga" width="112px">
						{#each manga.top as t, i (t.id)}
							{@const src = img(t.poster_path, 'w185')}
							<a href={titleHref(t)} class="top">
								<span class="cover">
									{#if src}<img {src} alt="" loading="lazy" />{/if}
									<span class="badge">{i + 1}</span>
								</span>
								<span class="top-name">{t.name}</span>
								<span class="faint top-meta">
									{#if t.rating !== null}<Score value={t.rating} />{/if}
									{#if t.rating !== null && t.chapters}·{/if}
									{#if t.chapters}{plural(t.chapters, 'chapter')}{/if}
								</span>
							</a>
						{/each}
					</Rail>
				</section>
			{/if}

			<div class="grid">
				{#if manga.genres.length}
					<section class="block card panel">
						<h2>Genres</h2>
						<ul class="hbars">
							{#each manga.genres.slice(0, 10) as g (g.name)}
								<li>
									<span class="hname">{g.name}</span>
									<span class="track"
										><span class="fill" style:width="{(g.titles / mangaGenreMax) * 100}%"
										></span></span
									>
									<span class="faint hcount">{g.titles}</span>
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				{#if manga.totals.rated}
					<section class="block card panel">
						<h2>Ratings</h2>
						<div class="columns" role="img" aria-label="How you rate manga">
							{#each manga.ratings as n, i (i)}
								<div
									class="column"
									title="{formatScore((i + 1) * 10, settings.current.scoreFormat)}: {n}"
								>
									<span class="value faint">{n || ''}</span>
									<span class="bar-v" style:height="{(n / mangaRatingMax) * 100}%"></span>
									<span class="label faint"><Score value={(i + 1) * 10} /></span>
								</div>
							{/each}
						</div>
					</section>
				{/if}

				{#if mangaStatusTotal > 0}
					<section class="block card panel">
						<h2>Status</h2>
						<div class="bar" role="img" aria-label="Manga by status">
							{#each STATUSES as s (s)}
								{@const n = manga.by_status[s] ?? 0}
								{#if n > 0}
									<span
										class="seg"
										data-status={s}
										style:flex-grow={n}
										title="{statusLabel(s, 'manga')}: {n}"
									></span>
								{/if}
							{/each}
						</div>
						<ul class="legend">
							{#each STATUSES as s (s)}
								{@const n = manga.by_status[s] ?? 0}
								{#if n > 0}
									<li>
										<span class="swatch" data-status={s}></span>
										{statusLabel(s, 'manga')}
										<span class="faint">{n}</span>
									</li>
								{/if}
							{/each}
						</ul>
					</section>
				{/if}
			</div>

			<div class="grid people-row">
				{@render people('Authors', manga.authors, '/person')}
			</div>
		</div>
	{/if}
{/snippet}

<div class="page">
	<div class="head page-head">
		<h1>Stats</h1>
		<div class="controls actions">
			{#if yearOptions.length > 1}
				<Select
					label="Period"
					value={shownYear ? String(shownYear) : 'all'}
					options={yearOptions}
					onchange={pickYear}
				/>
			{/if}
			{#if reviewTarget && half === 'watching'}
				<button class="btn primary" onclick={() => (reviewYear = reviewTarget)}>
					<Icon name="sparkle" size={17} />
					{reviewTarget} in review
				</button>
			{/if}
		</div>
	</div>

	{#if hasManga}
		<div class="halves" role="tablist" aria-label="What to count">
			<button
				role="tab"
				aria-selected={half === 'watching'}
				class:on={half === 'watching'}
				onclick={() => pickHalf('watching')}>Films &amp; shows</button
			>
			<button
				role="tab"
				aria-selected={half === 'manga'}
				class:on={half === 'manga'}
				onclick={() => pickHalf('manga')}>Manga</button
			>
		</div>
	{/if}

	{#if half === 'manga'}
		{@render mangaHalf()}
	{:else if loading && !stats}
		<Spinner label="Counting" />
	{:else if failed || !stats}
		{#if failed}<LoadFailed {failed} onretry={() => tries++} />{/if}
	{:else if stats.totals.titles === 0}
		<Empty title="Nothing watched yet" hint="Mark something watched and it shows up here.">
			<a class="btn primary" href="/search">Find something</a>
		</Empty>
	{:else}
		<div class="body" class:dim={loading}>
			<div class="tiles">
				<div class="tile card hero">
					<span class="eyebrow">Time watched</span>
					<strong>{longRuntime(stats.totals.minutes)}</strong>
					<span class="faint">{Math.round(stats.totals.minutes / 60).toLocaleString()} hours</span>
				</div>
				<div class="tile card">
					<span class="eyebrow">Films</span>
					<strong>{stats.totals.films.toLocaleString()}</strong>
					{#if stats.totals.rewatches}
						<span class="faint">{plural(stats.totals.rewatches, 'rewatch', 'rewatches')}</span>
					{/if}
				</div>
				<div class="tile card">
					<span class="eyebrow">Episodes</span>
					<strong>{stats.totals.episodes.toLocaleString()}</strong>
					<span class="faint">{plural(stats.totals.shows, 'show')}</span>
				</div>
				<div class="tile card">
					<span class="eyebrow">Days watching</span>
					<strong>{stats.totals.days.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Longest streak</span>
					<strong>{plural(stats.streaks.longest.days, 'day')}</strong>
					{#if stats.streaks.longest.days > 1}
						<span class="faint">
							{shortDate(stats.streaks.longest.from)} – {shortDate(stats.streaks.longest.to)}
						</span>
					{/if}
				</div>
				{#if stats.streaks.current.days > 0}
					<div class="tile card">
						<span class="eyebrow">Current streak</span>
						<strong class="streak"
							><Icon name="flame" size={22} />{plural(stats.streaks.current.days, 'day')}</strong
						>
					</div>
				{/if}
				<div class="tile card">
					<span class="eyebrow">Mean rating</span>
					<strong><Score value={stats.totals.mean_rating} /></strong>
					{#if rated}<span class="faint">{plural(rated, 'rating')}</span>{/if}
				</div>
			</div>

			{#if stats.timeline.some((t) => t.minutes > 0)}
				<section class="block card panel">
					<h2>{stats.year ? 'Watch time by month' : 'Watch time by year'}</h2>
					<div
						class="columns"
						class:wide={!stats.year}
						role="img"
						aria-label="Watch time over time"
					>
						{#each stats.timeline as point (point.unit)}
							<div
								class="column"
								title="{unitLabel(point.unit)}: {longRuntime(point.minutes)} · {plural(
									point.films,
									'film'
								)} · {plural(point.episodes, 'episode')}"
							>
								<!-- Hours, but under one hour a bar says its minutes rather than 0. -->
								<span class="value faint"
									>{!point.minutes
										? ''
										: point.minutes < 60
											? `${point.minutes}m`
											: Math.round(point.minutes / 60)}</span
								>
								<span class="bar-v" style:height="{(point.minutes / timelineMax) * 100}%"></span>
								<span class="label faint">{unitLabel(point.unit)}</span>
							</div>
						{/each}
					</div>
					<p class="faint note">Hours</p>
				</section>
			{/if}

			<div class="split">
				{@render titles('Most watched shows', stats.top_shows, true)}
				{@render titles('Top films', stats.top_films, false)}
			</div>

			<div class="grid">
				{#if stats.genres.length}
					<section class="block card panel">
						<h2>Genres</h2>
						<ul class="hbars">
							{#each stats.genres.slice(0, 10) as g (g.name)}
								<li>
									<span class="hname">{g.name}</span>
									<span class="track"
										><span class="fill" style:width="{(g.titles / genreMax) * 100}%"></span></span
									>
									<span class="faint hcount">{g.titles}</span>
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				{#if rated}
					<section class="block card panel">
						<h2>Ratings</h2>
						<div class="columns" role="img" aria-label="How you rate">
							{#each stats.ratings as n, i (i)}
								<div
									class="column"
									title="{formatScore((i + 1) * 10, settings.current.scoreFormat)}: {n}"
								>
									<span class="value faint">{n || ''}</span>
									<span class="bar-v" style:height="{(n / ratingMax) * 100}%"></span>
									<span class="label faint"><Score value={(i + 1) * 10} /></span>
								</div>
							{/each}
						</div>
					</section>
				{/if}

				{#if stats.decades.length}
					<section class="block card panel">
						<h2>Release decades</h2>
						<div class="columns" role="img" aria-label="Titles by release decade">
							{#each stats.decades as d (d.decade)}
								<div class="column" title="{d.decade}s: {plural(d.titles, 'title')}">
									<span class="value faint">{d.titles}</span>
									<span class="bar-v" style:height="{(d.titles / decadeMax) * 100}%"></span>
									<span class="label faint">{String(d.decade).slice(2)}s</span>
								</div>
							{/each}
						</div>
					</section>
				{/if}
			</div>

			<!-- The people get a row of their own, side by side, rather than
			     filling whatever gap the charts above leave. -->
			<div class="grid people-row">
				{@render people('Actors', stats.people.actors, '/person')}
				{@render people('Directors and creators', stats.people.directors, '/person')}
				{@render people('Studios', stats.people.studios, '/studio', true)}
			</div>

			{#if stats.credits_pending > 0}
				<p class="faint note">
					Cast and crew for {plural(stats.credits_pending, 'more title')} are on their way.
				</p>
			{/if}

			{#if !stats.year && library && statusTotal > 0}
				<section class="block card panel">
					<h2>Library</h2>
					<div class="bar" role="img" aria-label="Library by status">
						{#each STATUSES as s (s)}
							{@const n = library[s] ?? 0}
							{#if n > 0}
								<span class="seg" data-status={s} style:flex-grow={n} title="{statusLabel(s)}: {n}"
								></span>
							{/if}
						{/each}
					</div>
					<ul class="legend">
						{#each STATUSES as s (s)}
							{@const n = library[s] ?? 0}
							{#if n > 0}
								<li>
									<span class="swatch" data-status={s}></span>
									{statusLabel(s)}
									<span class="faint">{n}</span>
								</li>
							{/if}
						{/each}
					</ul>
				</section>
			{/if}
		</div>
	{/if}
</div>

{#if reviewYear}
	<YearInReview
		year={reviewYear}
		stats={stats?.year === reviewYear ? stats : null}
		onclose={() => (reviewYear = null)}
	/>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 14px;
		margin-bottom: 22px;
	}

	/* Films and shows, or manga: two halves, as AniList keeps them. */
	.halves {
		display: inline-flex;
		gap: 2px;
		padding: 3px;
		margin-bottom: 22px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
	}

	.halves button {
		padding: 7px 16px;
		border: 0;
		border-radius: var(--radius-pill);
		background: transparent;
		color: var(--text-dim);
		font-size: 0.86rem;
		font-weight: 650;
		cursor: pointer;
	}

	.halves button.on {
		background: var(--accent);
		color: var(--accent-ink);
	}

	.controls {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.body {
		transition: opacity var(--fast) var(--ease);
	}

	.body.dim {
		opacity: 0.55;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 12px;
		margin-bottom: 28px;
	}

	.tile {
		padding: 16px 18px;
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
	}

	.tile strong {
		font-family: var(--font-display);
		font-size: 1.8rem;
		font-weight: 650;
		letter-spacing: -0.03em;
		line-height: 1.1;
	}

	.tile.hero strong {
		color: var(--accent);
	}

	.tile .faint {
		font-size: 0.76rem;
	}

	.streak {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.streak :global(.icon) {
		color: var(--warn);
	}

	.block {
		margin-bottom: 18px;
	}

	.panel {
		padding: 18px;
		min-width: 0;
	}

	.panel h2 {
		font-size: 1.05rem;
		margin-bottom: 14px;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
		gap: 0 18px;
		align-items: start;
	}

	/* Three across, or one under the other: never two and a straggler. */
	.people-row {
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}

	@media (max-width: 900px) {
		.people-row {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	/* One under the other, each the full width: side by side, both were cut. */
	.split {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		margin: 10px 0 8px;
	}

	/* Vertical bars: timeline, ratings, decades. */
	.columns {
		display: flex;
		align-items: stretch;
		gap: 6px;
		height: 160px;
	}

	.column {
		flex: 1 1 0;
		min-width: 0;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		align-items: center;
		gap: 5px;
	}

	.columns.wide .column {
		max-width: 64px;
	}

	.bar-v {
		width: 100%;
		max-width: 38px;
		min-height: 2px;
		border-radius: 6px 6px 2px 2px;
		background: var(--accent);
		opacity: 0.85;
		transition: height var(--slow) var(--ease);
	}

	.column:hover .bar-v {
		opacity: 1;
	}

	.value,
	.label {
		font-size: 0.7rem;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.note {
		font-size: 0.76rem;
		margin-top: 10px;
	}

	/* Horizontal bars: genres. */
	.hbars {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 9px;
	}

	.hbars li {
		display: grid;
		grid-template-columns: minmax(80px, 34%) 1fr 34px;
		align-items: center;
		gap: 10px;
		font-size: 0.86rem;
	}

	.hname {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.track {
		height: 10px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.fill {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--accent);
	}

	.hcount {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	/* Ranked titles */
	.top {
		display: flex;
		flex-direction: column;
		gap: 4px;
		color: inherit;
		text-decoration: none;
	}

	.cover {
		position: relative;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		overflow: hidden;
		border: 1px solid var(--line);
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.badge {
		position: absolute;
		left: 6px;
		top: 6px;
		min-width: 22px;
		height: 22px;
		padding: 0 6px;
		display: grid;
		place-items: center;
		border-radius: var(--radius-pill);
		background: var(--scrim);
		backdrop-filter: blur(6px);
		font-size: 0.72rem;
		font-weight: 700;
	}

	.top-name {
		font-size: 0.82rem;
		font-weight: 600;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.top-meta {
		font-size: 0.72rem;
	}

	/* Ranked people */
	.people {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.person {
		display: grid;
		grid-template-columns: 18px 38px 1fr auto;
		align-items: center;
		gap: 10px;
		padding: 5px 6px;
		border-radius: var(--radius-sm);
		color: inherit;
		text-decoration: none;
		font-size: 0.88rem;
	}

	a.person:hover {
		background: var(--bg-elev-2);
	}

	.rank {
		color: var(--text-faint);
		font-size: 0.76rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
	}

	.avatar {
		width: 38px;
		aspect-ratio: 2 / 3;
		border-radius: 7px;
		overflow: hidden;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.72rem;
		font-weight: 700;
	}

	.avatar img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}

	/* Studio logos are mostly dark marks on transparent, so give them paper. */
	.avatar.logo {
		height: 38px;
		aspect-ratio: auto;
		border-radius: 9px;
		background: #f4f2ea;
		color: #3a3a3a;
	}

	.avatar.logo img {
		object-fit: contain;
		padding: 5px;
	}

	.name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.count {
		font-size: 0.76rem;
	}

	/* Library by status */
	.bar {
		display: flex;
		height: 14px;
		border-radius: var(--radius-pill);
		overflow: hidden;
		background: var(--bg-elev-2);
		gap: 2px;
	}

	.seg,
	.swatch {
		background: var(--status, var(--accent));
	}

	.legend {
		list-style: none;
		padding: 0;
		margin: 14px 0 0;
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
		font-size: 0.84rem;
	}

	.legend li {
		display: flex;
		align-items: center;
		gap: 7px;
	}

	.swatch {
		width: 10px;
		height: 10px;
		border-radius: 3px;
	}

	@media (max-width: 560px) {
		.tiles {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.tile.hero {
			grid-column: span 2;
		}

		.columns {
			gap: 3px;
			height: 130px;
		}

		.value {
			font-size: 0.6rem;
		}

		.label {
			font-size: 0.62rem;
		}
	}
</style>
