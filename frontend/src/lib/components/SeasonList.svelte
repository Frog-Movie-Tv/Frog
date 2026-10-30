<script lang="ts">
	import Icon from './Icon.svelte';
	import { api, img } from '#lib/api';
	import { episodeCode, prettyDate, relativeDate, runtime } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { whenWatched, type WhenAnswer } from '#lib/when.svelte';
	import type {
		EpisodeView,
		LibraryItem,
		RatingUnit,
		SeasonDetail,
		SeasonSummary
	} from '#lib/types';
	import Rating from './Rating.svelte';

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
		ratings = [],
		onprogress
	}: Props = $props();

	// `undefined` means "nothing clicked yet", so the default can keep tracking
	// the props without freezing their first value.
	let chosen = $state<number | null | undefined>(undefined);
	const open = $derived(
		chosen === undefined ? (initialSeason ?? seasons[0]?.season_number ?? null) : chosen
	);
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
		try {
			const detail = await api.season(titleId, season);
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
		try {
			const { item } = await api.setEpisodeWatched(
				titleId,
				ep.season_number,
				ep.episode_number,
				next
			);
			onprogress?.(item);
		} catch (e) {
			patch(ep.season_number, ep.episode_number, {
				watched: ep.watched,
				watch_count: ep.watch_count
			});
			toasts.error(e);
		} finally {
			busy = null;
		}

		// Unticking is the one that stings, so it is the one that offers undo.
		if (!next) {
			toasts.undoable(`Unmarked ${episodeCode(ep.season_number, ep.episode_number)}`, () =>
				toggle({ ...ep, watched: false })
			);
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

		busy = `season-${season}`;
		try {
			const { item } = await api.setSeasonWatched(titleId, season, watched, when);
			onprogress?.(item);
			await fetchSeason(season);
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

	const today = new Date().toISOString().slice(0, 10);

	/** TMDB gives an air date; anything later than today has not happened. */
	function unaired(ep: EpisodeView): boolean {
		return !!ep.air_date && ep.air_date > today;
	}

	function seasonWatched(season: number): boolean {
		const detail = loaded[season];
		return !!detail?.episodes.length && detail.episodes.every((e) => e.watched);
	}
</script>

<div class="seasons">
	{#each seasons as season (season.season_number)}
		{@const isOpen = open === season.season_number}
		{@const detail = loaded[season.season_number]}
		{@const units = unitsOf(season.season_number)}
		{@const whole = units.length === 1 && units[0].first_episode === 1 ? units[0] : null}
		{@const started = !!season.air_date && season.air_date <= today}
		<section class="season" class:open={isOpen}>
			<header>
				<button
					class="head"
					aria-expanded={isOpen}
					onclick={() => (chosen = isOpen ? null : season.season_number)}
				>
					<span class="chev" class:turned={isOpen}><Icon name="chevron-right" size={17} /></span>
					<span class="title">{season.name}</span>
					<span class="meta faint">
						{season.episode_count}
						{season.episode_count === 1 ? 'episode' : 'episodes'}{#if season.air_date}&nbsp;·
							{season.air_date.slice(0, 4)}{/if}
					</span>
				</button>

				{#if whole && (whole.rateable || started)}
					<div class="season-rating" class:unrated={ratingOf(whole) === null}>
						{#if whole.rateable}
							<Rating compact value={ratingOf(whole)} onchange={(v) => rateUnit(whole, v)} />
						{:else}
							<span class="wait faint" title="A season can be rated once every episode is out">
								Rate when it ends
							</span>
						{/if}
					</div>
				{/if}
				{#if isOpen && detail}
					<button
						class="btn ghost small season-action"
						disabled={busy === `season-${season.season_number}`}
						onclick={() => toggleSeason(season.season_number, !seasonWatched(season.season_number))}
					>
						<Icon name={seasonWatched(season.season_number) ? 'x' : 'check'} size={14} />
						{seasonWatched(season.season_number) ? 'Unmark' : 'Mark watched'}
					</button>
				{/if}
			</header>

			{#if isOpen && units.length > 1}
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

			{#if isOpen}
				{#if !detail}
					<ul class="episodes" aria-busy="true" aria-label="Loading episodes">
						{#each Array(Math.min(season.episode_count || 6, 8)) as _, i (i)}
							<li class="episode skeleton-row">
								<span class="check skeleton"></span>
								<span class="still skeleton"></span>
								<span class="body">
									<span class="line-skel skeleton"></span>
									<span class="line-skel short skeleton"></span>
								</span>
							</li>
						{/each}
					</ul>
				{:else}
					<ul class="episodes">
						{#each detail.episodes as ep (ep.episode_number)}
							{@const key = `${ep.season_number}-${ep.episode_number}`}
							<li class="episode" class:watched={ep.watched} class:future={unaired(ep)}>
								<button
									class="check"
									class:on={ep.watched}
									disabled={busy === key || unaired(ep)}
									aria-pressed={ep.watched}
									aria-label={unaired(ep)
										? `${episodeCode(ep.season_number, ep.episode_number)} has not aired yet`
										: `Mark ${episodeCode(ep.season_number, ep.episode_number)} watched`}
									title={unaired(ep) ? 'Not out yet' : 'Mark watched'}
									onclick={() => toggle(ep)}
								>
									<Icon name="check" size={15} strokeWidth={2.6} />
								</button>

								<div class="still" class:blurred={blurStill(ep)}>
									{#if ep.still_path}
										<img src={img(ep.still_path, 'w300')} alt="" loading="lazy" />
									{:else if fallbackStill}
										<img class="stand-in" src={img(fallbackStill, 'w300')} alt="" loading="lazy" />
									{:else}
										<span class="no-still" aria-hidden="true">
											{episodeCode(ep.season_number, ep.episode_number)}
										</span>
									{/if}
								</div>

								<div class="body">
									<div class="line">
										<span class="code faint">
											{episodeCode(ep.season_number, ep.episode_number)}
										</span>
										<span class="name">{ep.name ?? 'Untitled'}</span>
										{#if ep.watch_count > 1}
											<span class="pill tiny">×{ep.watch_count}</span>
										{/if}
									</div>
									<div class="sub faint">
										{#if unaired(ep)}
											<span class="soon">
												Airs {relativeDate(ep.air_date)} · {prettyDate(ep.air_date)}
											</span>
										{:else}
											{#if ep.air_date}<span>{prettyDate(ep.air_date)}</span>{/if}
											{#if ep.runtime}<span>{runtime(ep.runtime)}</span>{/if}
											{#if ep.notes}<span class="has-note">note</span>{/if}
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

								<div class="controls">
									{#if !unaired(ep)}
										<button
											class="note-btn"
											aria-label="Episode note"
											title="Episode note"
											onclick={() => (noteFor = noteFor === key ? null : key)}
										>
											<Icon name="pencil" size={15} />
										</button>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</section>
	{/each}
</div>

<style>
	.seasons {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	/* Not clipped: a season's rating opens a popover past its own edge. */
	.season {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
	}

	.season-rating {
		flex: none;
		display: flex;
		align-items: center;
	}

	.wait {
		font-size: 0.74rem;
		white-space: nowrap;
	}

	.parts {
		list-style: none;
		margin: 0 10px 8px;
		padding: 4px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		display: flex;
		flex-direction: column;
	}

	.parts li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 0;
	}

	.parts li + li {
		border-top: 1px solid var(--line);
	}

	.part-name {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		font-size: 0.85rem;
	}

	.part-name strong {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.part-name .faint {
		font-size: 0.74rem;
	}

	.season.open {
		border-color: var(--line-strong);
	}

	header {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0 10px;
		padding-right: 12px;
	}

	.season-action {
		flex: none;
	}

	.skeleton-row .check {
		border: none;
	}

	.line-skel {
		display: block;
		height: 11px;
		width: 62%;
		border-radius: 4px;
	}

	.line-skel.short {
		width: 34%;
	}

	.head {
		flex: 1 1 220px;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
	}

	.chev {
		display: inline-flex;
		font-size: 1.1rem;
		color: var(--text-faint);
		transition: transform var(--fast) var(--ease);
	}

	.chev.turned {
		transform: rotate(90deg);
	}

	.title {
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}

	/* One unbreakable unit, pushed to the right: "25 episodes · 2014" never
	   splits across two lines. */
	.meta {
		margin-left: auto;
		font-size: 0.78rem;
		white-space: nowrap;
		flex: none;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.78rem;
	}

	.episodes {
		list-style: none;
		margin: 0;
		padding: 0 10px 10px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.episode {
		display: grid;
		grid-template-columns: 26px 96px minmax(0, 1fr) auto;
		align-items: center;
		gap: 12px;
		padding: 8px 10px;
		border-radius: var(--radius-sm);
		transition: background var(--fast) var(--ease);
	}

	.episode:hover {
		background: var(--bg-elev-2);
	}

	.episode.watched .name {
		color: var(--text-dim);
	}

	.check {
		width: 26px;
		height: 26px;
		flex: none;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1.5px solid var(--line-strong);
		background: transparent;
		color: transparent;
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			border-color var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.check:hover {
		border-color: var(--accent);
		color: var(--text-faint);
	}

	.check.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-ink);
	}

	.check:active {
		transform: scale(0.88);
	}

	/* Always present, even without artwork: a collapsing column was letting
	   the rating control stretch across the whole row. */
	.still {
		width: 96px;
		flex: none;
		aspect-ratio: 16 / 9;
		border-radius: 6px;
		overflow: hidden;
		background: var(--bg-elev-2);
		display: grid;
		place-items: center;
	}

	.still img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	/* The show's backdrop where the episode has no still: faded, so a page of
	   identical pictures reads as a placeholder rather than as content. */
	.still .stand-in {
		opacity: 0.4;
		filter: saturate(0.4);
	}

	.still.blurred img {
		filter: blur(10px);
		transform: scale(1.1);
	}

	.no-still {
		font-size: 0.66rem;
		font-weight: 650;
		letter-spacing: 0.04em;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.episode.future {
		opacity: 0.68;
	}

	.episode.future .check {
		cursor: not-allowed;
		border-style: dashed;
	}

	.soon {
		color: var(--warn);
	}

	.body {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.code {
		font-variant-numeric: tabular-nums;
		font-size: 0.74rem;
		flex: none;
	}

	.name {
		font-size: 0.9rem;
		font-weight: 550;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.sub {
		display: flex;
		gap: 10px;
		font-size: 0.73rem;
	}

	.has-note {
		color: var(--accent);
	}

	.pill.tiny {
		padding: 1px 7px;
		font-size: 0.67rem;
	}

	.note {
		margin-top: 8px;
		resize: vertical;
	}

	.controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.note-btn {
		background: none;
		border: none;
		color: var(--text-faint);
		cursor: pointer;
		font-size: 0.95rem;
		padding: 4px;
	}

	.note-btn:hover {
		color: var(--text);
	}

	@media (max-width: 720px) {
		.still {
			display: none;
		}

		/* The name keeps the whole first line; the rating and the action drop
		   under it rather than squeezing it or pushing past the edge. */
		.head {
			flex-basis: 100%;
		}

		.season-rating,
		.season-action {
			margin: -4px 0 12px 43px;
		}

		.season-rating + .season-action {
			margin-left: 0;
		}

		/* A closed season only shows a rating it already has: a row of empty
		   chips down a long show is noise. */
		.season:not(.open) .season-rating.unrated {
			display: none;
		}

		.episode {
			grid-template-columns: 26px minmax(0, 1fr) auto;
		}
	}
</style>
