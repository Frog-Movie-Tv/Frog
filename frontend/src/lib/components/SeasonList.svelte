<script lang="ts">
	import Icon from './Icon.svelte';
	import { api, img } from '#lib/api';
	import { episodeCode, prettyDate, relativeDate, runtime } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { EpisodeView, Kind, LibraryItem, SeasonDetail, SeasonSummary } from '#lib/types';
	import Rating from './Rating.svelte';
	import Spinner from './Spinner.svelte';

	interface Props {
		kind: Kind;
		tmdbId: number;
		seasons: SeasonSummary[];
		/** Which season to open on first paint, usually the one you are on. */
		initialSeason?: number | null;
		onprogress?: (item: LibraryItem) => void;
	}

	let { kind, tmdbId, seasons, initialSeason = null, onprogress }: Props = $props();

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

	$effect(() => {
		if (open !== null && !loaded[open] && loading !== open) void fetchSeason(open);
	});

	async function fetchSeason(season: number) {
		loading = season;
		try {
			const detail = await api.season(kind, tmdbId, season);
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
				kind,
				tmdbId,
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
			toasts.undoable(
				`Unmarked ${episodeCode(ep.season_number, ep.episode_number)}`,
				() => toggle({ ...ep, watched: false })
			);
		}
	}

	async function toggleSeason(season: number, watched: boolean) {
		busy = `season-${season}`;
		try {
			const { item } = await api.setSeasonWatched(kind, tmdbId, season, watched);
			onprogress?.(item);
			await fetchSeason(season);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = null;
		}
	}

	async function rate(ep: EpisodeView, rating: number | null) {
		patch(ep.season_number, ep.episode_number, { rating });
		try {
			await api.setEpisodeState(kind, tmdbId, ep.season_number, ep.episode_number, {
				rating,
				notes: ep.notes
			});
		} catch (e) {
			patch(ep.season_number, ep.episode_number, { rating: ep.rating });
			toasts.error(e);
		}
	}

	async function saveNote(ep: EpisodeView, notes: string) {
		const trimmed = notes.trim() || null;
		patch(ep.season_number, ep.episode_number, { notes: trimmed });
		noteFor = null;
		try {
			await api.setEpisodeState(kind, tmdbId, ep.season_number, ep.episode_number, {
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
		<section class="season" class:open={isOpen}>
			<header>
				<button
					class="head"
					aria-expanded={isOpen}
					onclick={() => (chosen = isOpen ? null : season.season_number)}
				>
					<span class="chev" class:turned={isOpen}><Icon name="chevron-right" size={17} /></span>
					<span class="title">{season.name}</span>
					<span class="count faint">{season.episode_count} ep</span>
					{#if season.air_date}
						<span class="aired faint">{season.air_date.slice(0, 4)}</span>
					{/if}
				</button>

				{#if isOpen && detail}
					<button
						class="btn ghost small"
						disabled={busy === `season-${season.season_number}`}
						onclick={() =>
							toggleSeason(season.season_number, !seasonWatched(season.season_number))}
					>
						{seasonWatched(season.season_number) ? 'Unmark season' : 'Mark season watched'}
					</button>
				{/if}
			</header>

			{#if isOpen}
				{#if loading === season.season_number && !detail}
					<Spinner label="Loading episodes" />
				{:else if detail}
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
											onblur={(e) => saveNote(ep, e.currentTarget.value)}
										></textarea>
									{/if}
								</div>

								<div class="controls">
									{#if !unaired(ep)}
										<Rating compact value={ep.rating} onchange={(v) => rate(ep, v)} />
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

	.season {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		overflow: hidden;
	}

	.season.open {
		border-color: var(--line-strong);
	}

	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding-right: 12px;
	}

	.head {
		flex: 1;
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
	}

	.count,
	.aired {
		font-size: 0.78rem;
	}

	.aired {
		margin-left: auto;
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

		.episode {
			grid-template-columns: 26px minmax(0, 1fr);
			grid-template-areas:
				'check body'
				'. controls';
			row-gap: 6px;
		}

		.check {
			grid-area: check;
		}
		.body {
			grid-area: body;
		}
		.controls {
			grid-area: controls;
			justify-content: flex-start;
		}
	}
</style>
