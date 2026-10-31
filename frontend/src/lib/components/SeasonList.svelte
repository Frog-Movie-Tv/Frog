<script lang="ts">
	import Icon from './Icon.svelte';
	import { api, img } from '#lib/api';
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

		// Unticking is the one that stings, so it is the one that offers undo,
		// and the undo puts the watch back on the day it was, not today.
		if (!next) {
			toasts.undoable(`Unmarked ${episodeCode(ep.season_number, ep.episode_number)}`, () =>
				restore(ep.season_number, [ep])
			);
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

	function seasonWatched(season: number): boolean {
		const detail = loaded[season];
		return !!detail?.episodes.length && detail.episodes.every((e) => e.watched);
	}
</script>

<div class="seasons">
	{#if ordered.length > 1}
		<div class="season-tabs" role="tablist" aria-label="Seasons">
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
						{season.episode_count === 1 ? 'episode' : 'episodes'}{#if season.air_date}
							· {season.air_date.slice(0, 4)}{/if}
					</span>
				</div>
				<div class="season-actions">
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
			{:else}
				<ul class="episodes">
					{#each uniqueBy(detail.episodes, (e) => e.episode_number) as ep (ep.episode_number)}
						{@const key = `${ep.season_number}-${ep.episode_number}`}
						{@const code = episodeCode(ep.season_number, ep.episode_number)}
						<li class="episode" class:watched={ep.watched} class:future={unaired(ep)}>
							<div class="still" class:blurred={blurStill(ep)}>
								{#if ep.still_path}
									<img src={img(ep.still_path, 'w300')} alt="" loading="lazy" />
								{:else if fallbackStill}
									<img class="stand-in" src={img(fallbackStill, 'w300')} alt="" loading="lazy" />
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
									<Icon name="check" size={16} strokeWidth={2.6} />
								</button>
								{#if ep.watch_count > 1}<span class="times">×{ep.watch_count}</span>{/if}
							</div>

							<div class="body">
								<div class="line">
									<span class="code">{code}</span>
									<span class="name">{ep.name ?? 'Untitled'}</span>
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
	.season-tabs {
		display: flex;
		gap: 6px;
		margin-bottom: 18px;
		overflow-x: auto;
		scrollbar-width: none;
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
		background: var(--bg-elev-2);
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
	}
</style>
