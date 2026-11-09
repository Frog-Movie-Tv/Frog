<script lang="ts">
	import Icon from './Icon.svelte';
	import { api, img, type ClearedWatch } from '#lib/api';
	import { localDay } from '#lib/day';
	import { episodeCode, prettyDate, relativeDate, runtime } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { uniqueBy } from '#lib/unique';
	import { whenWatched, type WhenAnswer } from '#lib/when.svelte';
	import type {
		EpisodeView,
		LibraryItem,
		RatingUnit,
		SeasonDetail,
		SeasonSummary
	} from '#lib/types';
	import Rating from './Rating.svelte';
	import Score from './Score.svelte';

	interface Props {
		/** Frog's id for the show. */
		titleId: number;
		seasons: SeasonSummary[];
		/** Which season to open on first paint, usually the one you are on. */
		initialSeason?: number | null;
		/** Shown in the "When did you watch this?" prompt. */
		showName?: string;
		/** The show's backdrop, for episodes TMDB has no still of. */
		fallbackStill?: string | null;
		/** Its poster, for a show with no backdrop either. */
		fallbackPoster?: string | null;
		/** What can be rated: whole seasons, or the AniList parts of one. */
		ratings?: RatingUnit[];
		onprogress?: (item: LibraryItem) => void;
	}

	let {
		titleId,
		seasons,
		initialSeason = null,
		showName = 'This show',
		fallbackStill = null,
		fallbackPoster = null,
		ratings = [],
		onprogress
	}: Props = $props();

	// `undefined` means "nothing clicked yet", so the default can keep tracking
	// the props without freezing their first value.
	let chosen = $state<number | undefined>(undefined);
	/** Seasons in order, specials last: they are rarely where you start. */
	const ordered = $derived(
		uniqueBy(
			[
				...seasons.filter((s) => s.season_number !== 0),
				...seasons.filter((s) => s.season_number === 0)
			],
			(s) => s.season_number
		)
	);
	const open = $derived(chosen ?? initialSeason ?? ordered[0]?.season_number ?? null);
	const current = $derived(ordered.find((s) => s.season_number === open) ?? null);
	let loaded = $state<Record<number, SeasonDetail>>({});
	let loading = $state<number | null>(null);
	let busy = $state<string | null>(null);
	let noteFor = $state<string | null>(null);
	/** Ratings changed here, ahead of the page reloading the title. */
	let rated = $state<Record<string, number | null>>({});

	const unitKey = (u: RatingUnit) => `${u.season}-${u.first_episode}`;
	const unitsOf = (season: number) => ratings.filter((u) => u.season === season);
	const ratingOf = (u: RatingUnit) => (unitKey(u) in rated ? rated[unitKey(u)] : u.rating);

	async function rateUnit(u: RatingUnit, rating: number | null) {
		const before = ratingOf(u);
		rated = { ...rated, [unitKey(u)]: rating };
		try {
			await api.rateSeason(titleId, u.season, u.first_episode, rating);
		} catch (e) {
			rated = { ...rated, [unitKey(u)]: before };
			toasts.error(e);
		}
	}

	$effect(() => {
		if (open !== null && !loaded[open] && loading !== open) void fetchSeason(open);
	});

	async function fetchSeason(season: number) {
		loading = season;
		// Another show's page may have taken this one's place by the time the
		// answer comes; its episodes must never land here.
		const forTitle = titleId;
		try {
			const detail = await api.season(forTitle, season);
			if (forTitle !== titleId) return;
			loaded = { ...loaded, [season]: detail };
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = null;
		}
	}

	/** Hide what happens next, when spoiler protection is on. */
	function blurStill(ep: EpisodeView): boolean {
		return settings.current.spoilerProtection && !ep.watched;
	}

	function patch(season: number, episode: number, changes: Partial<EpisodeView>) {
		const detail = loaded[season];
		if (!detail) return;
		loaded = {
			...loaded,
			[season]: {
				...detail,
				episodes: detail.episodes.map((e) =>
					e.episode_number === episode ? { ...e, ...changes } : e
				)
			}
		};
	}

	async function toggle(ep: EpisodeView) {
		const key = `${ep.season_number}-${ep.episode_number}`;
		busy = key;
		const next = !ep.watched;
		// Optimistic: the checkmark is the most-used control in the app and must
		// never feel like it is waiting on the network.
		patch(ep.season_number, ep.episode_number, {
			watched: next,
			watch_count: next ? Math.max(1, ep.watch_count) : 0
		});
		let cleared: ClearedWatch[] = [];
		try {
			const answer = await api.setEpisodeWatched(
				titleId,
				ep.season_number,
				ep.episode_number,
				next
			);
			cleared = answer.cleared ?? [];
			onprogress?.(answer.item);
		} catch (e) {
			patch(ep.season_number, ep.episode_number, {
				watched: ep.watched,
				watch_count: ep.watch_count
			});
			toasts.error(e);
		} finally {
			busy = null;
		}

		// Unticking is the one that stings, so it is the one that offers undo,
		// and the undo puts back every watch of it on the day it was, rewatches
		// too, not one today.
		if (!next) {
			toasts.undoable(`Unmarked ${episodeCode(ep.season_number, ep.episode_number)}`, () =>
				cleared.length ? restoreAll(ep, cleared) : restore(ep.season_number, [ep])
			);
		}
	}

	/** Every watch an untick cleared, back as it was. */
	async function restoreAll(ep: EpisodeView, cleared: ClearedWatch[]) {
		busy = `season-${ep.season_number}`;
		try {
			const { item } = await api.setEpisodeWatched(
				titleId,
				ep.season_number,
				ep.episode_number,
				true,
				undefined,
				cleared
			);
			onprogress?.(item);
			await fetchSeason(ep.season_number);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = null;
		}
	}

	/** Put back watches that were cleared, each with its own date. */
	async function restore(season: number, episodes: EpisodeView[]) {
		busy = `season-${season}`;
		try {
			let item: LibraryItem | undefined;
			for (const ep of episodes) {
				({ item } = await api.setEpisodeWatched(
					titleId,
					ep.season_number,
					ep.episode_number,
					true,
					{ watched_at: ep.watched_at, undated: ep.undated }
				));
			}
			if (item) onprogress?.(item);
			await fetchSeason(season);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = null;
		}
	}

	async function toggleSeason(season: number, watched: boolean) {
		// Marking a season is a claim about the past, so ask when. Clearing it
		// needs no date.
		let when: WhenAnswer | undefined;
		if (watched) {
			const name = seasons.find((s) => s.season_number === season)?.name ?? `Season ${season}`;
			const answer = await whenWatched.ask(showName, name);
			if (!answer) return;
			when = answer;
		}

		const cleared = watched ? [] : (loaded[season]?.episodes.filter((e) => e.watched) ?? []);
		busy = `season-${season}`;
		try {
			const { item } = await api.setSeasonWatched(titleId, season, watched, when);
			onprogress?.(item);
			await fetchSeason(season);
			if (cleared.length) {
				const name = seasons.find((s) => s.season_number === season)?.name ?? `Season ${season}`;
				toasts.undoable(`Unmarked ${name}`, () => restore(season, cleared));
			}
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = null;
		}
	}

	async function saveNote(ep: EpisodeView, notes: string) {
		const trimmed = notes.trim() || null;
		patch(ep.season_number, ep.episode_number, { notes: trimmed });
		noteFor = null;
		try {
			await api.setEpisodeState(titleId, ep.season_number, ep.episode_number, {
				rating: ep.rating,
				notes: trimmed
			});
			toasts.push('Note saved');
		} catch (e) {
			toasts.error(e);
		}
	}

	const today = localDay();

	/** TMDB gives an air date; anything later than today has not happened. */
	function unaired(ep: EpisodeView): boolean {
		return !!ep.air_date && ep.air_date > today;
	}

	/** Cards with stills, or a compact list: one choice, for every show. */
	const view = $derived(settings.current.episodeView ?? 'grid');

	/** What stands in for a missing still: the backdrop, else the poster. */
	const standIn = $derived(
		fallbackStill ? img(fallbackStill, 'w300') : fallbackPoster ? img(fallbackPoster, 'w342') : null
	);

	/*
	 * A long show has more seasons than fit across. The row scrolls, and
	 * arrows say there is more and move to it; the open season is kept in view.
	 */
	let tabsEl: HTMLDivElement | undefined = $state();
	let canLeft = $state(false);
	let canRight = $state(false);

	function measure() {
		if (!tabsEl) return;
		canLeft = tabsEl.scrollLeft > 4;
		canRight = tabsEl.scrollLeft + tabsEl.clientWidth < tabsEl.scrollWidth - 4;
	}

	function nudge(direction: -1 | 1) {
		if (!tabsEl) return;
		tabsEl.scrollBy({ left: direction * tabsEl.clientWidth * 0.75, behavior: 'smooth' });
	}

	$effect(() => {
		const el = tabsEl;
		if (!el) return;
		measure();
		const resize = new ResizeObserver(measure);
		resize.observe(el);
		// A mouse wheel moves the row sideways, as a trackpad already does.
		const wheel = (e: WheelEvent) => {
			if (el.scrollWidth <= el.clientWidth || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
			el.scrollLeft += e.deltaY;
			e.preventDefault();
		};
		el.addEventListener('wheel', wheel, { passive: false });
		return () => {
			resize.disconnect();
			el.removeEventListener('wheel', wheel);
		};
	});

	$effect(() => {
		void open;
		const el = tabsEl;
		const on = el?.querySelector<HTMLElement>('.season-tab.on');
		if (!el || !on) return;
		const left = on.offsetLeft - el.offsetLeft;
		if (left < el.scrollLeft || left + on.offsetWidth > el.scrollLeft + el.clientWidth) {
			el.scrollTo({ left: left - el.clientWidth / 2 + on.offsetWidth / 2 });
		}
		measure();
	});

	function seasonWatched(season: number): boolean {
		const detail = loaded[season];
		return !!detail?.episodes.length && detail.episodes.every((e) => e.watched);
	}
</script>

<!-- A rating an episode was given before seasons were rated instead: kept,
     shown, and no longer changed here. -->
{#snippet oldRating(ep: EpisodeView)}
	{#if ep.rating !== null}
		<span class="ep-score" title="Your rating for this episode"><Score value={ep.rating} /></span>
	{/if}
{/snippet}

<div class="seasons">
	{#if ordered.length > 1}
		<div class="tabs-wrap" class:fade-left={canLeft} class:fade-right={canRight}>
			{#if canLeft}
				<button class="tab-arrow left" aria-label="Earlier seasons" onclick={() => nudge(-1)}>
					<Icon name="chevron-left" size={18} />
				</button>
			{/if}
			<div
				class="season-tabs"
				role="tablist"
				aria-label="Seasons"
				bind:this={tabsEl}
				onscroll={measure}
			>
				{#each ordered as season (season.season_number)}
					<button
						role="tab"
						class="season-tab"
						class:on={open === season.season_number}
						aria-selected={open === season.season_number}
						onclick={() => (chosen = season.season_number)}
					>
						{season.name}
					</button>
				{/each}
			</div>
			{#if canRight}
				<button class="tab-arrow right" aria-label="Later seasons" onclick={() => nudge(1)}>
					<Icon name="chevron-right" size={18} />
				</button>
			{/if}
		</div>
	{/if}

	{#if current}
		{@const season = current}
		{@const detail = loaded[season.season_number]}
		{@const units = unitsOf(season.season_number)}
		{@const whole = units.length === 1 && units[0].first_episode === 1 ? units[0] : null}
		{@const started = !!season.air_date && season.air_date <= today}
		<section class="season" aria-label={season.name}>
			<header class="season-head">
				<div class="season-title">
					<h3>{season.name}</h3>
					<span class="faint">
						{season.episode_count}
						{season.episode_count === 1 ? 'episode' : 'episodes'}
						{#if season.air_date}· {season.air_date.slice(0, 4)}{/if}
					</span>
				</div>
				<div class="season-actions">
					<div class="view-switch" role="group" aria-label="Show episodes as">
						<button
							class:on={view === 'grid'}
							aria-pressed={view === 'grid'}
							title="Cards"
							aria-label="Cards"
							onclick={() => settings.set('episodeView', 'grid')}
						>
							<Icon name="grid" size={15} />
						</button>
						<button
							class:on={view === 'list'}
							aria-pressed={view === 'list'}
							title="List"
							aria-label="List"
							onclick={() => settings.set('episodeView', 'list')}
						>
							<Icon name="list" size={16} />
						</button>
					</div>
					{#if whole && (whole.rateable || started)}
						{#if whole.rateable}
							<Rating compact value={ratingOf(whole)} onchange={(v) => rateUnit(whole, v)} />
						{:else}
							<span class="wait faint" title="A season can be rated once every episode is out">
								Rate when it ends
							</span>
						{/if}
					{/if}
					{#if detail}
						<button
							class="btn small"
							disabled={busy === `season-${season.season_number}`}
							onclick={() =>
								toggleSeason(season.season_number, !seasonWatched(season.season_number))}
						>
							<Icon name={seasonWatched(season.season_number) ? 'x' : 'check'} size={14} />
							{seasonWatched(season.season_number) ? 'Unmark' : 'Mark watched'}
						</button>
					{/if}
				</div>
			</header>

			{#if units.length > 1}
				<ul class="parts" aria-label="Parts of {season.name}">
					{#each units as u (unitKey(u))}
						<li>
							<span class="part-name">
								<strong>{u.name ?? `From episode ${u.first_episode}`}</strong>
								<span class="faint">
									{u.episodes === 1
										? `Episode ${u.first_episode}`
										: `Episodes ${u.first_episode}–${u.first_episode + u.episodes - 1}`}
								</span>
							</span>
							{#if u.rateable}
								<Rating compact value={ratingOf(u)} onchange={(v) => rateUnit(u, v)} />
							{:else}
								<span class="wait faint" title="A part can be rated once every episode is out">
									Rate when it ends
								</span>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}

			{#if !detail}
				<ul class="episodes" aria-busy="true" aria-label="Loading episodes">
					{#each Array(Math.min(season.episode_count || 6, 8)) as _, i (i)}
						<li class="episode">
							<span class="still skeleton"></span>
							<span class="line-skel skeleton"></span>
							<span class="line-skel short skeleton"></span>
						</li>
					{/each}
				</ul>
			{:else if view === 'list'}
				<ul class="ep-list">
					{#each uniqueBy(detail.episodes, (e) => e.episode_number) as ep (ep.episode_number)}
						{@const key = `${ep.season_number}-${ep.episode_number}`}
						{@const code = episodeCode(ep.season_number, ep.episode_number)}
						<li class="ep-row" class:watched={ep.watched} class:future={unaired(ep)}>
							<button
								class="check row-check hit"
								class:on={ep.watched}
								disabled={busy === key || unaired(ep)}
								aria-pressed={ep.watched}
								aria-label={unaired(ep) ? `${code} has not aired yet` : `Mark ${code} watched`}
								title={unaired(ep) ? 'Not out yet' : 'Mark watched'}
								onclick={() => toggle(ep)}
							>
								<Icon name="check" size={14} />
							</button>
							<span class="code">{ep.episode_number}</span>
							<div class="row-body">
								<span class="name">
									{ep.name ?? 'Untitled'}
									{#if ep.watch_count > 1}<span class="times-inline">×{ep.watch_count}</span>{/if}
									{@render oldRating(ep)}
								</span>
								{#if ep.overview && !blurStill(ep)}
									<p class="overview">{ep.overview}</p>
								{/if}
								{#if noteFor === key}
									<textarea
										class="input note"
										rows="3"
										placeholder="What did you think?"
										value={ep.notes ?? ''}
										onblur={(e) => saveNote(ep, e.currentTarget.value)}></textarea>
								{/if}
							</div>
							<span class="row-meta">
								{#if unaired(ep)}
									<span class="soon">Airs {relativeDate(ep.air_date)}</span>
								{:else}
									{#if ep.air_date}<span>{prettyDate(ep.air_date)}</span>{/if}
									{#if ep.runtime}<span>{runtime(ep.runtime)}</span>{/if}
								{/if}
							</span>
							{#if !unaired(ep)}
								<button
									class="note-btn hit"
									class:has={!!ep.notes}
									aria-label="Episode note"
									title={ep.notes ? 'Your note' : 'Add a note'}
									onclick={() => (noteFor = noteFor === key ? null : key)}
								>
									<Icon name="pencil" size={13} />
								</button>
							{/if}
						</li>
					{/each}
				</ul>
			{:else}
				<ul class="episodes">
					{#each uniqueBy(detail.episodes, (e) => e.episode_number) as ep (ep.episode_number)}
						{@const key = `${ep.season_number}-${ep.episode_number}`}
						{@const code = episodeCode(ep.season_number, ep.episode_number)}
						<li class="episode" class:watched={ep.watched} class:future={unaired(ep)}>
							<div class="still" class:blurred={blurStill(ep)}>
								{#if ep.still_path}
									<img src={img(ep.still_path, 'w300')} alt="" loading="lazy" />
								{:else if standIn}
									<img class="stand-in" src={standIn} alt="" loading="lazy" />
								{:else}
									<span class="no-still" aria-hidden="true">{code}</span>
								{/if}
								<button
									class="check hit"
									class:on={ep.watched}
									disabled={busy === key || unaired(ep)}
									aria-pressed={ep.watched}
									aria-label={unaired(ep) ? `${code} has not aired yet` : `Mark ${code} watched`}
									title={unaired(ep) ? 'Not out yet' : 'Mark watched'}
									onclick={() => toggle(ep)}
								>
									<Icon name="check" size={16} />
								</button>
								{#if ep.watch_count > 1}<span class="times">×{ep.watch_count}</span>{/if}
							</div>

							<div class="body">
								<div class="line">
									<span class="code">{code}</span>
									<span class="name">{ep.name ?? 'Untitled'}</span>
									{@render oldRating(ep)}
								</div>
								<div class="sub">
									{#if unaired(ep)}
										<span class="soon">Airs {relativeDate(ep.air_date)}</span>
									{:else}
										{#if ep.air_date}<span>{prettyDate(ep.air_date)}</span>{/if}
										{#if ep.runtime}<span>{runtime(ep.runtime)}</span>{/if}
									{/if}
									{#if !unaired(ep)}
										<button
											class="note-btn hit"
											class:has={!!ep.notes}
											aria-label="Episode note"
											title={ep.notes ? 'Your note' : 'Add a note'}
											onclick={() => (noteFor = noteFor === key ? null : key)}
										>
											<Icon name="pencil" size={13} />
										</button>
									{/if}
								</div>
								{#if noteFor === key}
									<textarea
										class="input note"
										rows="3"
										placeholder="What did you think?"
										value={ep.notes ?? ''}
										onblur={(e) => saveNote(ep, e.currentTarget.value)}></textarea>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>

<style>
	.tabs-wrap {
		position: relative;
		margin-bottom: 18px;
	}

	.season-tabs {
		display: flex;
		gap: 6px;
		overflow-x: auto;
		scrollbar-width: none;
		scroll-behavior: smooth;
	}

	/* The row fades where it goes on, so a cut-off season reads as "more". */
	.tabs-wrap.fade-right .season-tabs {
		mask-image: linear-gradient(to right, #000 calc(100% - 64px), transparent);
	}

	.tabs-wrap.fade-left .season-tabs {
		mask-image: linear-gradient(to left, #000 calc(100% - 64px), transparent);
	}

	.tabs-wrap.fade-left.fade-right .season-tabs {
		mask-image: linear-gradient(
			to right,
			transparent,
			#000 64px,
			#000 calc(100% - 64px),
			transparent
		);
	}

	.tab-arrow {
		position: absolute;
		top: 50%;
		z-index: 1;
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		margin-top: -17px;
		border: 1px solid var(--line-strong);
		border-radius: 50%;
		background: var(--bg-elev);
		color: var(--text);
		box-shadow: var(--shadow-2);
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.tab-arrow:hover {
		transform: scale(1.08);
	}

	.tab-arrow.left {
		left: -4px;
	}

	.tab-arrow.right {
		right: -4px;
	}

	.view-switch {
		display: inline-flex;
		padding: 3px;
		gap: 2px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
	}

	.view-switch button {
		display: grid;
		place-items: center;
		width: 30px;
		height: 26px;
		border: 0;
		border-radius: var(--radius-pill);
		background: none;
		color: var(--text-faint);
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.view-switch button:hover {
		color: var(--text);
	}

	.view-switch button.on {
		background: var(--bg-elev-2);
		color: var(--text);
	}

	/* The list: one line an episode, the tick first, quick to run down. */
	.ep-list {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--line);
	}

	.ep-row {
		display: grid;
		grid-template-columns: auto 2.4ch minmax(0, 1fr) auto auto;
		align-items: start;
		gap: 4px 14px;
		padding: 12px 4px;
		border-bottom: 1px solid var(--line);
		transition: background var(--fast) var(--ease);
	}

	.ep-row:hover {
		background: color-mix(in srgb, var(--bg-elev) 60%, transparent);
	}

	.ep-row .code {
		padding-top: 6px;
		text-align: right;
	}

	.ep-row .name {
		display: block;
		padding-top: 4px;
		white-space: normal;
	}

	.ep-row.watched .name {
		color: var(--text-dim);
	}

	.ep-row.future {
		opacity: 0.6;
	}

	.ep-row .row-check {
		position: static;
		width: 28px;
		height: 28px;
		border-color: var(--line-strong);
		background: none;
	}

	.ep-row .row-check:hover:not(:disabled) {
		color: var(--text-faint);
	}

	.ep-row .row-check.on {
		border-color: transparent;
		background: var(--accent);
		color: var(--accent-ink);
	}

	.row-body {
		min-width: 0;
		overflow-wrap: normal;
	}

	.overview {
		margin-top: 4px;
		font-size: 0.8rem;
		line-height: 1.45;
		color: var(--text-faint);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.row-meta {
		display: flex;
		gap: 10px;
		padding-top: 6px;
		font-size: 0.76rem;
		color: var(--text-faint);
		white-space: nowrap;
	}

	.ep-row .note-btn {
		margin-top: 2px;
	}

	.ep-score {
		margin-left: 6px;
		font-size: 0.72rem;
		font-weight: 650;
		color: var(--text-dim);
		white-space: nowrap;
	}

	.times-inline {
		margin-left: 6px;
		font-size: 0.7rem;
		font-weight: 650;
		color: var(--accent);
	}

	.season-tabs::-webkit-scrollbar {
		display: none;
	}

	.season-tab {
		flex: none;
		height: 36px;
		padding: 0 16px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: none;
		color: var(--text-dim);
		font-size: 0.86rem;
		font-weight: 600;
		cursor: pointer;
		transition:
			color var(--fast) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.season-tab:hover {
		color: var(--text);
		border-color: var(--line-strong);
	}

	.season-tab.on {
		border-color: transparent;
		background: var(--text);
		color: var(--bg);
	}

	.season-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 10px 16px;
		margin-bottom: 16px;
	}

	.season-title {
		display: flex;
		align-items: baseline;
		gap: 12px;
	}

	.season-title h3 {
		font-size: 1.1rem;
	}

	.season-title .faint {
		font-size: 0.84rem;
	}

	.season-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.btn.small {
		min-height: 34px;
		padding: 0 14px;
		font-size: 0.82rem;
	}

	.wait {
		font-size: 0.78rem;
	}

	.parts {
		list-style: none;
		margin: 0 0 18px;
		padding: 0;
		display: grid;
		gap: 2px;
	}

	.parts li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}

	.part-name {
		display: grid;
		gap: 1px;
		min-width: 0;
		font-size: 0.88rem;
	}

	.part-name .faint {
		font-size: 0.76rem;
	}

	/* Episodes as cards: the still says which one, the tick on it says
	   whether you have seen it. */
	.episodes {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 22px 16px;
	}

	.episode {
		display: flex;
		flex-direction: column;
		gap: 9px;
		min-width: 0;
	}

	.still {
		position: relative;
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.still img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.still .stand-in {
		opacity: 0.45;
	}

	/* Spoiler guard, from Settings: unseen stills are blurred. */
	.still.blurred img {
		filter: blur(14px);
		transform: scale(1.1);
	}

	.no-still {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--text-faint);
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}

	.episode.watched .still img {
		opacity: 0.55;
	}

	.episode.future .still {
		opacity: 0.6;
	}

	.check {
		position: absolute;
		right: 8px;
		bottom: 8px;
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border: 1.5px solid rgb(255 255 255 / 0.85);
		border-radius: 50%;
		background: rgb(0 0 0 / 0.45);
		color: transparent;
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.check:hover:not(:disabled) {
		color: rgb(255 255 255 / 0.85);
	}

	.check.on {
		border-color: transparent;
		background: var(--accent);
		color: var(--accent-ink);
	}

	.check:disabled {
		cursor: default;
		opacity: 0.5;
	}

	.times {
		position: absolute;
		left: 8px;
		top: 8px;
		padding: 1px 7px;
		border-radius: var(--radius-pill);
		background: rgb(0 0 0 / 0.6);
		color: #fff;
		font-size: 0.7rem;
		font-weight: 650;
	}

	.body {
		display: grid;
		gap: 3px;
		min-width: 0;
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.code {
		flex: none;
		color: var(--text-faint);
		font-size: 0.74rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.name {
		font-size: 0.9rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.sub {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 22px;
		font-size: 0.76rem;
		color: var(--text-faint);
	}

	.soon {
		color: var(--accent);
		font-weight: 600;
	}

	.note-btn {
		display: grid;
		place-items: center;
		width: 24px;
		height: 22px;
		margin-left: auto;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--text-faint);
		cursor: pointer;
	}

	.note-btn:hover {
		color: var(--text);
		background: var(--bg-hover);
	}

	.note-btn.has {
		color: var(--accent);
	}

	.note {
		margin-top: 6px;
		font-size: 0.86rem;
	}

	.line-skel {
		display: block;
		height: 11px;
		width: 80%;
	}

	.line-skel.short {
		width: 45%;
	}

	/* A phone: rows, the still small on the left, so a season is quick to
	   tick through with a thumb. */
	@media (max-width: 600px) {
		.episodes {
			grid-template-columns: minmax(0, 1fr);
			gap: 12px;
		}

		.episode {
			display: grid;
			grid-template-columns: 128px minmax(0, 1fr);
			align-items: center;
			gap: 12px;
		}

		.check {
			right: 6px;
			bottom: 6px;
			width: 30px;
			height: 30px;
		}

		.ep-row {
			grid-template-columns: auto 2.4ch minmax(0, 1fr) auto;
		}

		.row-meta {
			grid-column: 3;
			grid-row: 2;
			padding-top: 0;
		}

		.ep-row .row-check {
			width: 30px;
			height: 30px;
		}
	}
</style>
