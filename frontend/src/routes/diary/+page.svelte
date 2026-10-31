<script lang="ts">
	import { titleHref } from '#lib/titles';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { episodeCode, formatScore, plural, prettyDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { whenWatched } from '#lib/when.svelte';

	interface Episode {
		season_number: number;
		episode_number: number;
		name: string | null;
		rating: number | null;
		rewatch: boolean;
	}

	interface Entry {
		day: string;
		watched_at: string;
		/** Frog's id for the title. */
		id: number;
		kind: 'movie' | 'tv';
		title: string;
		poster_path: string | null;
		film: boolean;
		count: number;
		rewatch: boolean;
		imported: boolean;
		rating: number | null;
		episodes: Episode[];
	}

	let entries = $state<Entry[]>([]);
	let total = $state(0);
	let undated = $state(0);
	let loading = $state(true);
	let loadingMore = $state(false);
	let open = $state<string | null>(null);

	$effect(() => {
		if (!session.authenticated) return;
		void load(0);
	});

	async function load(offset: number) {
		if (offset > 0) loadingMore = true;
		try {
			const res = await fetch(`/api/diary?limit=60&offset=${offset}`, {
				credentials: 'same-origin'
			});
			if (!res.ok) throw new Error('Could not load your diary');
			const data = await res.json();
			entries = offset === 0 ? data.entries : [...entries, ...data.entries];
			total = data.total;
			undated = data.undated;
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = false;
			loadingMore = false;
		}
	}

	const keyOf = (e: Entry) => `${e.day}-${e.id}-${e.film}`;

	const months = $derived.by(() => {
		const out: { key: string; label: string; entries: Entry[] }[] = [];
		for (const entry of entries) {
			const key = entry.day.slice(0, 7);
			let bucket = out.find((m) => m.key === key);
			if (!bucket) {
				const date = new Date(`${key}-01T00:00:00Z`);
				bucket = {
					key,
					label: Number.isNaN(date.getTime())
						? key
						: date.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }),
					entries: []
				};
				out.push(bucket);
			}
			bucket.entries.push(entry);
		}
		return out;
	});

	/**
	 * "S04E08" for one, "S04E08–E12" for a run, "S01E01–E04, E29–E38" for
	 * two runs with a gap. More gaps than that and only the ends are shown,
	 * beside the count.
	 */
	function range(episodes: Episode[]): string {
		if (!episodes.length) return '';
		const runs: [Episode, Episode][] = [];
		for (const ep of episodes) {
			const run = runs.at(-1);
			if (
				run &&
				run[1].season_number === ep.season_number &&
				run[1].episode_number + 1 === ep.episode_number
			) {
				run[1] = ep;
			} else {
				runs.push([ep, ep]);
			}
		}
		const shown = runs.length <= 2 ? runs : [[runs[0][0], runs[runs.length - 1][1]] as const];
		let season: number | null = null;
		return shown
			.map(([a, b]) => {
				const start =
					a.season_number === season
						? `E${String(a.episode_number).padStart(2, '0')}`
						: episodeCode(a.season_number, a.episode_number);
				season = b.season_number;
				if (a === b) return start;
				const end =
					b.season_number === a.season_number
						? `E${String(b.episode_number).padStart(2, '0')}`
						: episodeCode(b.season_number, b.episode_number);
				return `${start}–${end}`;
			})
			.join(', ');
	}

	async function changeDate(entry: Entry) {
		const what = entry.film ? 'Film' : plural(entry.count, 'episode');
		const answer = await whenWatched.ask(entry.title, `${what} on ${prettyDate(entry.day)}`, {
			allowToday: false,
			film: entry.film,
			day: entry.day,
			confirm: 'Change date'
		});
		if (!answer) return;
		try {
			const { changed } = await api.redateDiary(entry.id, entry.day, answer);
			toasts.push(
				answer.when === 'unknown'
					? `${plural(changed, 'watch', 'watches')} moved out of the diary`
					: `${plural(changed, 'watch', 'watches')} redated`
			);
			open = null;
			await load(0);
		} catch (e) {
			toasts.error(e);
		}
	}

	const fmt = $derived(settings.current.scoreFormat);

	/** "Mon", in the reader's language, for a YYYY-MM-DD day. */
	function weekday(day: string): string {
		return new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { weekday: 'short' });
	}
</script>

<svelte:head>
	<title>Diary · Frog</title>
</svelte:head>

<div class="page">
	<div class="diary">
		<div class="page-head">
			<div>
				<h1>Diary</h1>
				{#if total}<p class="sub">{plural(total, 'entry', 'entries')}</p>{/if}
			</div>
		</div>

		{#if loading}
			<Spinner label="Reading your diary" />
		{:else if !entries.length}
			<Empty
				art="rest"
				title="Nothing watched yet"
				hint={undated
					? `${plural(undated, 'watch', 'watches')} without a date are counted, but not shown here.`
					: 'Tick an episode or mark a film watched and it will show up here.'}
			>
				<a class="btn primary" href="/library">Open library</a>
			</Empty>
		{:else}
			{#each months as month (month.key)}
				<section class="month">
					<h2>{month.label}</h2>
					<ol class="entries">
						{#each month.entries as entry, i (keyOf(entry))}
							{@const key = keyOf(entry)}
							{@const expandable = entry.episodes.length > 1}
							{@const newDay = i === 0 || month.entries[i - 1].day !== entry.day}
							<li class="entry" class:new-day={newDay}>
								<!-- The day once, at the first thing watched on it. -->
								<span class="date" aria-hidden={!newDay}>
									{#if newDay}
										<strong>{Number(entry.day.slice(8, 10))}</strong>
										<small>{weekday(entry.day)}</small>
									{/if}
								</span>

								<div class="body">
									<div class="row">
										<a class="art" href={titleHref(entry)} aria-label={entry.title}>
											{#if entry.poster_path}
												<img src={img(entry.poster_path, 'w92')} alt="" loading="lazy" />
											{:else}
												<Icon name={entry.film ? 'film' : 'tv'} size={16} />
											{/if}
										</a>

										<button
											class="what"
											aria-expanded={expandable ? open === key : undefined}
											disabled={!expandable}
											onclick={() => (open = open === key ? null : key)}
										>
											<span class="line">
												<strong>{entry.title}</strong>
												{#if entry.rewatch}<span class="tag">Rewatch</span>{/if}
												{#if entry.imported}<span class="tag">Imported</span>{/if}
											</span>
											<span class="detail">
												{#if entry.film}
													Film
												{:else if entry.episodes.length === 1}
													{range(entry.episodes)}{#if entry.episodes[0].name}
														· {entry.episodes[0].name}{/if}
												{:else}
													{range(entry.episodes)} · {plural(entry.count, 'episode')}
													<Icon name={open === key ? 'chevron-up' : 'chevron-down'} size={13} />
												{/if}
											</span>
										</button>

										{#if entry.film && entry.rating !== null}
											<span class="score">{formatScore(entry.rating, fmt)}</span>
										{:else if entry.episodes.length === 1 && entry.episodes[0].rating !== null}
											<span class="score">{formatScore(entry.episodes[0].rating, fmt)}</span>
										{/if}

										<button
											class="icon-btn hit"
											aria-label="Change date for {entry.title} on {entry.day}"
											title="Change date"
											onclick={() => changeDate(entry)}
										>
											<Icon name="calendar" size={15} />
										</button>
									</div>

									{#if expandable && open === key}
										<ul class="episodes">
											{#each entry.episodes as ep, n (n)}
												<li>
													<span class="code"
														>{episodeCode(ep.season_number, ep.episode_number)}</span
													>
													<span class="ep-name">{ep.name ?? 'Untitled'}</span>
													{#if ep.rating !== null}
														<span class="score small">{formatScore(ep.rating, fmt)}</span>
													{/if}
												</li>
											{/each}
										</ul>
									{/if}
								</div>
							</li>
						{/each}
					</ol>
				</section>
			{/each}

			{#if entries.length < total}
				<div class="more">
					<button class="btn" disabled={loadingMore} onclick={() => load(entries.length)}>
						{loadingMore ? 'Loading' : 'Show more'}
					</button>
				</div>
			{/if}

			{#if undated}
				<p class="undated">
					{plural(undated, 'watch', 'watches')} without a date are counted, but not shown here.
				</p>
			{/if}
		{/if}
	</div>
</div>

<style>
	/* A diary is read down the middle of the page, like a journal. */
	.diary {
		max-width: 760px;
		margin: 0 auto;
	}

	.month {
		margin-bottom: 36px;
	}

	.month h2 {
		position: sticky;
		top: var(--header-h);
		z-index: 5;
		padding: 12px 0 10px;
		background: var(--bg);
		border-bottom: 1px solid var(--line);
		font-size: 1.05rem;
	}

	.entries {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.entry {
		display: grid;
		grid-template-columns: 52px minmax(0, 1fr);
		gap: 0 12px;
	}

	/* A new day draws a line across; a second watch that day does not. */
	.entry.new-day .body {
		border-top: 1px solid var(--line);
	}

	.entries > .entry:first-child .body {
		border-top: 0;
	}

	.date {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding-top: 14px;
		line-height: 1.1;
	}

	.date strong {
		font-family: var(--font-display);
		font-size: 1.35rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
	}

	.date small {
		font-size: 0.7rem;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-faint);
	}

	.body {
		min-width: 0;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 10px 0;
	}

	.art {
		width: 40px;
		aspect-ratio: 2 / 3;
		border-radius: 5px;
		overflow: hidden;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-faint);
		flex: none;
	}

	.art img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 0;
		border: none;
		background: none;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}

	.what:disabled {
		cursor: default;
	}

	.what:not(:disabled):hover .detail {
		color: var(--text);
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.line strong {
		font-size: 0.95rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tag {
		flex: none;
		font-size: 0.68rem;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-faint);
	}

	.detail {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.8rem;
		color: var(--text-dim);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.score {
		flex: none;
		font-family: var(--font-display);
		font-weight: 650;
		font-size: 0.95rem;
		font-variant-numeric: tabular-nums;
	}

	.score.small {
		font-size: 0.82rem;
	}

	.icon-btn {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 0;
		background: none;
		color: var(--text-faint);
		cursor: pointer;
		flex: none;
	}

	.icon-btn:hover {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.episodes {
		list-style: none;
		margin: 0 0 10px 54px;
		padding: 4px 0 4px 14px;
		border-left: 2px solid var(--line);
		max-height: 340px;
		overflow-y: auto;
	}

	.episodes li {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 5px 0;
		font-size: 0.84rem;
	}

	.code {
		flex: none;
		color: var(--text-faint);
		font-size: 0.76rem;
		font-variant-numeric: tabular-nums;
	}

	.ep-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.more {
		display: flex;
		justify-content: center;
		padding: 10px 0 20px;
	}

	.undated {
		text-align: center;
		font-size: 0.8rem;
		color: var(--text-faint);
		padding-bottom: 20px;
	}

	@media (max-width: 560px) {
		.entry {
			grid-template-columns: 40px minmax(0, 1fr);
			gap: 0 8px;
		}

		.episodes {
			margin-left: 0;
		}
	}
</style>
