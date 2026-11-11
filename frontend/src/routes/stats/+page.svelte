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
	import type {
		FullStats,
		MangaStats,
		StatsNarrow,
		StatsPerson,
		StatsTitle,
		Status
	} from '#lib/types';
	import { STATUSES, statusLabel } from '#lib/types';
	import {
		decadeOf,
		mangaView,
		toggle,
		toggleManga,
		view,
		type Dimension,
		type MangaDimension,
		type MangaFilter,
		type StatsFilter
	} from '#lib/stats-filter';

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

	/*
	 * The manga half is a dashboard too: a bar picked narrows the rest, from
	 * one row per manga (`mangaView`). Days reading and the authors come from
	 * the server for the manga a pick keeps (`narrowed people` below).
	 */
	let mangaFilter = $state<MangaFilter>({});
	$effect(() => {
		void yearParam;
		void half;
		mangaFilter = {};
	});
	const mangaPicked = $derived(Object.keys(mangaFilter).length > 0);
	const mangaNarrowed = $derived(
		manga && mangaPicked && manga.facts
			? mangaView(
					manga.facts,
					mangaFilter,
					manga.timeline.map((t) => t.unit),
					manga.year !== null
				)
			: null
	);
	function pickManga<K extends MangaDimension>(key: K, value: MangaFilter[K]) {
		mangaFilter = toggleManga(mangaFilter, key, value);
	}
	function unpickManga(key: MangaDimension) {
		const next = { ...mangaFilter };
		delete next[key];
		mangaFilter = next;
	}
	const mOn = (key: MangaDimension, value: unknown) => mangaFilter[key] === value;
	const mOff = (key: MangaDimension, value: unknown) =>
		mangaFilter[key] !== undefined && mangaFilter[key] !== value;

	const mTotals = $derived(
		mangaNarrowed?.totals ??
			manga?.totals ?? { manga: 0, chapters: 0, completed: 0, mean_rating: null, rated: 0 }
	);
	const mTimeline = $derived(mangaNarrowed?.timeline ?? manga?.timeline ?? []);
	const mGenres = $derived(mangaNarrowed?.genres ?? manga?.genres ?? []);
	const mRatings = $derived(mangaNarrowed?.ratings ?? manga?.ratings ?? []);
	const mStatus = $derived<Partial<Record<string, number>>>(
		mangaNarrowed?.by_status ?? manga?.by_status ?? {}
	);
	const mTop = $derived(mangaNarrowed?.top ?? manga?.top ?? []);

	const chaptersMax = $derived(Math.max(1, ...mTimeline.map((t) => t.chapters)));
	const mangaGenreMax = $derived(Math.max(1, ...mGenres.map((g) => g.titles)));
	const mangaRatingMax = $derived(Math.max(1, ...mRatings));
	const mangaStatusTotal = $derived(STATUSES.reduce((sum, s) => sum + (mStatus[s] ?? 0), 0));

	/** What is picked in the manga half, in words. */
	const mangaChips = $derived.by(() => {
		const out: { key: MangaDimension; label: string }[] = [];
		if (mangaFilter.unit) out.push({ key: 'unit', label: unitName(mangaFilter.unit) });
		if (mangaFilter.genre) out.push({ key: 'genre', label: mangaFilter.genre });
		if (mangaFilter.rating !== undefined) {
			out.push({
				key: 'rating',
				label: `Rated ${formatScore((mangaFilter.rating + 1) * 10, settings.current.scoreFormat)}`
			});
		}
		if (mangaFilter.status) {
			out.push({ key: 'status', label: statusLabel(mangaFilter.status as Status, 'manga') });
		}
		return out;
	});

	/** The year the review button makes: the one in view, else the latest. */
	const reviewTarget = $derived(stats?.year ?? stats?.years[0] ?? null);

	const statusTotal = $derived(STATUSES.reduce((sum, s) => sum + (library?.[s as Status] ?? 0), 0));

	const MONTHS = Array.from({ length: 12 }, (_, i) =>
		new Date(2000, i, 1).toLocaleDateString(undefined, { month: 'short' })
	);

	function unitLabel(unit: string): string {
		return unit.length === 7 ? (MONTHS[Number(unit.slice(5)) - 1] ?? unit) : unit;
	}

	/*
	 * A dashboard: a bar clicked narrows every other chart to the titles under
	 * it, and picks combine. A new period starts with nothing picked.
	 */
	let filter = $state<StatsFilter>({});
	$effect(() => {
		void yearParam;
		filter = {};
	});
	const picked = $derived(Object.keys(filter).length > 0);
	const narrowed = $derived(
		stats && picked && stats.facts
			? view(
					stats.facts,
					filter,
					stats.timeline.map((t) => t.unit)
				)
			: null
	);
	function pick<K extends Dimension>(key: K, value: StatsFilter[K]) {
		filter = toggle(filter, key, value);
	}
	/** This bar is the one picked, or another in its chart is. */
	const isOn = (key: Dimension, value: unknown) => filter[key] === value;
	const isOff = (key: Dimension, value: unknown) =>
		filter[key] !== undefined && filter[key] !== value;

	const totals = $derived(
		narrowed?.totals ?? {
			...(stats?.totals ?? {
				films: 0,
				episodes: 0,
				shows: 0,
				titles: 0,
				rewatches: 0,
				minutes: 0,
				days: 0,
				mean_rating: null
			}),
			rated: (stats?.ratings ?? []).reduce((a, b) => a + b, 0)
		}
	);
	const timeline = $derived(narrowed?.timeline ?? stats?.timeline ?? []);
	const genres = $derived(narrowed?.genres ?? stats?.genres ?? []);
	const decades = $derived(narrowed?.decades ?? stats?.decades ?? []);
	const ratingBars = $derived(narrowed?.ratings ?? stats?.ratings ?? []);
	const topShows = $derived(narrowed?.top_shows ?? stats?.top_shows ?? []);
	const topFilms = $derived(narrowed?.top_films ?? stats?.top_films ?? []);

	/*
	 * Days, streaks and people need every watch and credit, which the facts do
	 * not carry: under a pick, the server counts them for the titles kept.
	 */
	let extra = $state<StatsNarrow | null>(null);
	let mangaExtra = $state<StatsNarrow | null>(null);
	function narrowFrom(
		half: 'watching' | 'manga',
		ids: number[] | undefined,
		unit: string | undefined,
		year: number | null,
		set: (n: StatsNarrow | null) => void
	) {
		if (!ids) {
			set(null);
			return;
		}
		let stale = false;
		const timer = setTimeout(() => {
			api
				.statsNarrow(half, ids, year ?? undefined, unit)
				.then((n) => !stale && set(n))
				.catch(() => !stale && set(null));
		}, 120);
		return () => {
			stale = true;
			clearTimeout(timer);
		};
	}
	$effect(() =>
		narrowFrom('watching', narrowed?.ids, filter.unit, stats?.year ?? null, (n) => (extra = n))
	);
	$effect(() =>
		narrowFrom(
			'manga',
			mangaNarrowed?.ids,
			mangaFilter.unit,
			manga?.year ?? null,
			(n) => (mangaExtra = n)
		)
	);
	const days = $derived(picked ? extra?.days : stats?.totals.days);
	const streaksShown = $derived(picked ? extra?.streaks : stats?.streaks);
	const peopleShown = $derived(picked ? (extra?.people ?? null) : (stats?.people ?? null));
	const mangaDays = $derived(mangaPicked ? mangaExtra?.days : manga?.totals.days);
	const authorsShown = $derived(
		mangaPicked ? (mangaExtra?.authors ?? null) : (manga?.authors ?? null)
	);

	const timelineMax = $derived(Math.max(1, ...timeline.map((t) => t.minutes)));
	const genreMax = $derived(Math.max(1, ...genres.map((g) => g.titles)));
	const decadeMax = $derived(Math.max(1, ...decades.map((d) => d.titles)));
	const ratingMax = $derived(Math.max(1, ...ratingBars));
	const rated = $derived(totals.rated);

	/** A ratings bar's label, short enough for ten of them side by side. */
	function axisScore(i: number): string {
		const format = settings.current.scoreFormat;
		if (format === 'star5') return String((i + 1) / 2);
		if (format === 'pt100') return String((i + 1) * 10);
		if (format === 'pt10dec') return ((i + 1) as number).toFixed(0);
		return formatScore((i + 1) * 10, format);
	}

	/** What is picked, in words, for the chips above the charts. */
	const chips = $derived.by(() => {
		const out: { key: Dimension; label: string }[] = [];
		if (filter.kind) out.push({ key: 'kind', label: filter.kind === 'movie' ? 'Films' : 'Shows' });
		if (filter.unit) out.push({ key: 'unit', label: unitName(filter.unit) });
		if (filter.genre) out.push({ key: 'genre', label: filter.genre });
		if (filter.decade !== undefined) out.push({ key: 'decade', label: `${filter.decade}s` });
		if (filter.rating !== undefined) {
			out.push({
				key: 'rating',
				label: `Rated ${formatScore((filter.rating + 1) * 10, settings.current.scoreFormat)}`
			});
		}
		return out;
	});

	function unitName(unit: string): string {
		if (unit.length !== 7) return unit;
		return new Date(`${unit}-01T12:00:00`).toLocaleDateString(undefined, {
			month: 'long',
			year: 'numeric'
		});
	}

	function unpick(key: Dimension) {
		const next = { ...filter };
		delete next[key];
		filter = next;
	}

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
					<strong>{mTotals.chapters.toLocaleString()}</strong>
					<span class="faint">{plural(mTotals.manga, 'manga', 'manga')}</span>
				</div>
				<div class="tile card">
					<span class="eyebrow">Completed</span>
					<strong>{mTotals.completed.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Days reading</span>
					<strong>{mangaDays === undefined ? '…' : mangaDays.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Mean rating</span>
					<strong><Score value={mTotals.mean_rating} /></strong>
					{#if mTotals.rated}<span class="faint">{plural(mTotals.rated, 'rating')}</span>{/if}
				</div>
			</div>

			<div class="picks" aria-live="polite">
				{#if mangaChips.length}
					{#each mangaChips as chip (chip.key)}
						<button type="button" class="chip on" onclick={() => unpickManga(chip.key)}>
							{chip.label}
							<Icon name="x" size={13} label="Clear {chip.label}" />
						</button>
					{/each}
					<button type="button" class="btn ghost small" onclick={() => (mangaFilter = {})}>
						Clear all
					</button>
				{:else if manga.facts?.length}
					<span class="faint">Pick any bar to see the rest of your stats through it.</span>
				{/if}
			</div>

			{#if mTimeline.some((t) => t.chapters > 0)}
				<section class="block card panel">
					<h2>{manga.year ? 'Chapters by month' : 'Chapters by year'}</h2>
					<div
						class="columns"
						class:wide={!manga.year}
						role="group"
						aria-label="Chapters read over time"
					>
						{#each mTimeline as point (point.unit)}
							<button
								type="button"
								class="column pick"
								class:on={mOn('unit', point.unit)}
								class:off={mOff('unit', point.unit)}
								aria-pressed={mOn('unit', point.unit)}
								disabled={!manga.facts || (!point.chapters && !mOn('unit', point.unit))}
								onclick={() => pickManga('unit', point.unit)}
								title="{unitLabel(point.unit)}: {plural(point.chapters, 'chapter')}"
							>
								<span class="value faint">{point.chapters || ''}</span>
								<span class="bar-v" style:height="{(point.chapters / chaptersMax) * 100}%"></span>
								<span class="label faint">{unitLabel(point.unit)}</span>
							</button>
						{/each}
					</div>
				</section>
			{/if}

			{#if mTop.length}
				<section class="block">
					<Rail title="Top manga" width="112px">
						{#each mTop as t, i (t.id)}
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
				{#if mGenres.length}
					<section class="block card panel">
						<h2>Genres</h2>
						<ul class="hbars">
							{#each mGenres.slice(0, 10) as g (g.name)}
								<li>
									<button
										type="button"
										class="hrow pick"
										class:on={mOn('genre', g.name)}
										class:off={mOff('genre', g.name)}
										aria-pressed={mOn('genre', g.name)}
										disabled={!manga.facts}
										onclick={() => pickManga('genre', g.name)}
									>
										<span class="hname">{g.name}</span>
										<span class="track"
											><span class="fill" style:width="{(g.titles / mangaGenreMax) * 100}%"
											></span></span
										>
										<span class="faint hcount">{g.titles}</span>
									</button>
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				{#if mRatings.some((n) => n > 0)}
					<section class="block card panel">
						<h2>Ratings</h2>
						<div class="columns" role="group" aria-label="How you rate manga">
							{#each mRatings as n, i (i)}
								<button
									type="button"
									class="column pick"
									class:on={mOn('rating', i)}
									class:off={mOff('rating', i)}
									aria-pressed={mOn('rating', i)}
									disabled={!manga.facts || (!n && !mOn('rating', i))}
									onclick={() => pickManga('rating', i)}
									title="{formatScore((i + 1) * 10, settings.current.scoreFormat)}: {n}"
								>
									<span class="value faint">{n || ''}</span>
									<span class="bar-v" style:height="{(n / mangaRatingMax) * 100}%"></span>
									<span class="label faint"><Score value={(i + 1) * 10} /></span>
								</button>
							{/each}
						</div>
					</section>
				{/if}

				{#if mangaStatusTotal > 0}
					<section class="block card panel">
						<h2>Status</h2>
						<div class="bar" role="group" aria-label="Manga by status">
							{#each STATUSES as s (s)}
								{@const n = mStatus[s] ?? 0}
								{#if n > 0}
									<button
										type="button"
										class="seg pick"
										class:on={mOn('status', s)}
										class:off={mOff('status', s)}
										aria-pressed={mOn('status', s)}
										aria-label="{statusLabel(s, 'manga')}: {n}"
										disabled={!manga.facts}
										data-status={s}
										style:flex-grow={n}
										title="{statusLabel(s, 'manga')}: {n}"
										onclick={() => pickManga('status', s)}
									></button>
								{/if}
							{/each}
						</div>
						<ul class="legend">
							{#each STATUSES as s (s)}
								{@const n = mStatus[s] ?? 0}
								{#if n > 0}
									<li>
										<button
											type="button"
											class="legend-pick pick"
											class:on={mOn('status', s)}
											class:off={mOff('status', s)}
											aria-pressed={mOn('status', s)}
											disabled={!manga.facts}
											onclick={() => pickManga('status', s)}
										>
											<span class="swatch" data-status={s}></span>
											{statusLabel(s, 'manga')}
											<span class="faint">{n}</span>
										</button>
									</li>
								{/if}
							{/each}
						</ul>
					</section>
				{/if}
			</div>

			<div class="grid people-row" class:settling={mangaPicked && !mangaExtra}>
				{@render people('Authors', authorsShown ?? manga.authors, '/person')}
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
					<strong>{longRuntime(totals.minutes)}</strong>
					<span class="faint">{Math.round(totals.minutes / 60).toLocaleString()} hours</span>
				</div>
				<button
					type="button"
					class="tile card pickable"
					class:on={isOn('kind', 'movie')}
					aria-pressed={isOn('kind', 'movie')}
					title="Only films"
					onclick={() => pick('kind', 'movie')}
				>
					<span class="eyebrow">Films</span>
					<strong>{totals.films.toLocaleString()}</strong>
					{#if totals.rewatches}
						<span class="faint">{plural(totals.rewatches, 'rewatch', 'rewatches')}</span>
					{/if}
				</button>
				<button
					type="button"
					class="tile card pickable"
					class:on={isOn('kind', 'tv')}
					aria-pressed={isOn('kind', 'tv')}
					title="Only shows"
					onclick={() => pick('kind', 'tv')}
				>
					<span class="eyebrow">Episodes</span>
					<strong>{totals.episodes.toLocaleString()}</strong>
					<span class="faint">{plural(totals.shows, 'show')}</span>
				</button>
				{#if picked}
					<div class="tile card">
						<span class="eyebrow">Titles</span>
						<strong>{totals.titles.toLocaleString()}</strong>
					</div>
				{/if}
				<div class="tile card">
					<span class="eyebrow">Days watching</span>
					<strong>{days === undefined ? '…' : days.toLocaleString()}</strong>
				</div>
				<div class="tile card">
					<span class="eyebrow">Longest streak</span>
					<strong>{streaksShown ? plural(streaksShown.longest.days, 'day') : '…'}</strong>
					{#if streaksShown && streaksShown.longest.days > 1}
						<span class="faint">
							{shortDate(streaksShown.longest.from)} – {shortDate(streaksShown.longest.to)}
						</span>
					{/if}
				</div>
				{#if streaksShown && streaksShown.current.days > 0}
					<div class="tile card">
						<span class="eyebrow">Current streak</span>
						<strong class="streak"
							><Icon name="flame" size={22} />{plural(streaksShown.current.days, 'day')}</strong
						>
					</div>
				{/if}
				<div class="tile card">
					<span class="eyebrow">Mean rating</span>
					<strong><Score value={totals.mean_rating} /></strong>
					{#if rated}<span class="faint">{plural(rated, 'rating')}</span>{/if}
				</div>
			</div>

			<div class="picks" aria-live="polite">
				{#if chips.length}
					{#each chips as chip (chip.key)}
						<button type="button" class="chip on" onclick={() => unpick(chip.key)}>
							{chip.label}
							<Icon name="x" size={13} label="Clear {chip.label}" />
						</button>
					{/each}
					<button type="button" class="btn ghost small" onclick={() => (filter = {})}>
						Clear all
					</button>
				{:else if stats.facts?.length}
					<span class="faint">Pick any bar to see the rest of your stats through it.</span>
				{/if}
			</div>

			{#if timeline.some((t) => t.minutes > 0)}
				<section class="block card panel">
					<h2>{stats.year ? 'Watch time by month' : 'Watch time by year'}</h2>
					<div
						class="columns"
						class:wide={!stats.year}
						role="img"
						aria-label="Watch time over time"
					>
						{#each timeline as point (point.unit)}
							<button
								type="button"
								class="column pick"
								class:on={isOn('unit', point.unit)}
								class:off={isOff('unit', point.unit)}
								aria-pressed={isOn('unit', point.unit)}
								disabled={!point.minutes && !isOn('unit', point.unit)}
								onclick={() => pick('unit', point.unit)}
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
							</button>
						{/each}
					</div>
					<p class="faint note">Hours</p>
				</section>
			{/if}

			<div class="split">
				{@render titles('Most watched shows', topShows, true)}
				{@render titles('Top films', topFilms, false)}
			</div>

			<div class="grid">
				{#if genres.length}
					<section class="block card panel">
						<h2>Genres</h2>
						<ul class="hbars">
							{#each genres.slice(0, 10) as g (g.name)}
								<li>
									<button
										type="button"
										class="hrow pick"
										class:on={isOn('genre', g.name)}
										class:off={isOff('genre', g.name)}
										aria-pressed={isOn('genre', g.name)}
										onclick={() => pick('genre', g.name)}
									>
										<span class="hname">{g.name}</span>
										<span class="track"
											><span class="fill" style:width="{(g.titles / genreMax) * 100}%"></span></span
										>
										<span class="faint hcount">{g.titles}</span>
									</button>
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				{#if rated}
					<section class="block card panel">
						<h2>Ratings</h2>
						<div class="columns" role="group" aria-label="How you rate">
							{#each ratingBars as n, i (i)}
								<button
									type="button"
									class="column pick"
									class:on={isOn('rating', i)}
									class:off={isOff('rating', i)}
									aria-pressed={isOn('rating', i)}
									disabled={!n && !isOn('rating', i)}
									onclick={() => pick('rating', i)}
									title="{formatScore((i + 1) * 10, settings.current.scoreFormat)}: {n}"
								>
									<span class="value faint">{n || ''}</span>
									<span class="bar-v" style:height="{(n / ratingMax) * 100}%"></span>
									<span class="label faint axis"
										>{#if settings.current.scoreFormat === 'smiley3'}<Score
												value={(i + 1) * 10}
											/>{:else}{axisScore(i)}{#if settings.current.scoreFormat === 'star5'}<Icon
													name="star"
													size={9}
													filled
												/>{/if}{/if}</span
									>
								</button>
							{/each}
						</div>
					</section>
				{/if}

				{#if decades.length}
					<section class="block card panel">
						<h2>Release decades</h2>
						<div class="columns" role="group" aria-label="Titles by release decade">
							{#each decades as d (d.decade)}
								<button
									type="button"
									class="column pick"
									class:on={isOn('decade', d.decade)}
									class:off={isOff('decade', d.decade)}
									aria-pressed={isOn('decade', d.decade)}
									onclick={() => pick('decade', decadeOf(d.decade))}
									title="{d.decade}s: {plural(d.titles, 'title')}"
								>
									<span class="value faint">{d.titles}</span>
									<span class="bar-v" style:height="{(d.titles / decadeMax) * 100}%"></span>
									<span class="label faint">{String(d.decade).slice(2)}s</span>
								</button>
							{/each}
						</div>
					</section>
				{/if}
			</div>

			<!-- The people get a row of their own, side by side, rather than
			     filling whatever gap the charts above leave. -->
			<div class="grid people-row" class:settling={picked && !extra}>
				{@render people('Actors', (peopleShown ?? stats.people).actors, '/person')}
				{@render people(
					'Directors and creators',
					(peopleShown ?? stats.people).directors,
					'/person'
				)}
				{@render people('Studios', (peopleShown ?? stats.people).studios, '/studio', true)}
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
	/* Waiting for the people under a new pick: the old ones, faded. */
	.settling {
		opacity: 0.5;
		transition: opacity var(--fast) var(--ease);
	}

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

	/* A bar that narrows the page when clicked. */
	.pick {
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
		border-radius: var(--radius-sm);
		transition: opacity var(--fast) var(--ease);
	}

	.pick:disabled {
		cursor: default;
	}

	.pick:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.pick.off {
		opacity: 0.35;
	}

	.pick.on .bar-v,
	.pick.on .fill {
		opacity: 1;
		box-shadow:
			0 0 0 2px var(--bg-float),
			0 0 0 4px var(--accent);
	}

	.pick.on .label,
	.pick.on .value,
	.pick.on .hname {
		color: var(--text);
		font-weight: 700;
	}

	.axis {
		display: inline-flex;
		align-items: center;
		gap: 1px;
	}

	.picks {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		min-height: 34px;
		margin: 4px 0 14px;
		font-size: 0.84rem;
	}

	.picks .chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	.tile.pickable {
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
		transition:
			box-shadow var(--fast) var(--ease),
			background var(--fast) var(--ease);
	}

	.tile.pickable:hover {
		background: var(--bg-hover);
	}

	.tile.pickable.on {
		box-shadow: inset 0 0 0 2px var(--accent);
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
		font-size: 0.86rem;
	}

	.hrow {
		display: grid;
		grid-template-columns: minmax(80px, 34%) 1fr 34px;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 3px 4px;
		margin: -3px -4px;
		text-align: left;
	}

	.hrow:hover {
		background: var(--bg-hover);
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
		background: var(--bg-hover);
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

	.legend-pick {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 2px 4px;
	}

	.legend-pick.on {
		font-weight: 700;
	}

	.seg.pick {
		border-radius: 0;
	}

	.seg.pick.on {
		box-shadow: inset 0 0 0 2px var(--bg-float);
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
