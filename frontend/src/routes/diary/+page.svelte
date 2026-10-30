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
</script>

<svelte:head>
	<title>Diary · Frog</title>
</svelte:head>

<div class="page">
	<div class="section-head">
		<h1>Diary</h1>
		{#if total}<span class="faint">{plural(total, 'entry', 'entries')}</span>{/if}
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
					{#each month.entries as entry (keyOf(entry))}
						{@const key = keyOf(entry)}
						{@const expandable = entry.episodes.length > 1}
						<li class="entry" class:open={open === key}>
							<span class="day faint">{entry.day.slice(8, 10)}</span>

							<div class="card-row">
								<div class="row">
									<a class="art" href={titleHref(entry)} aria-label={entry.title}>
										{#if entry.poster_path}
											<img src={img(entry.poster_path, 'w92')} alt="" loading="lazy" />
										{:else}
											<Icon name={entry.film ? 'film' : 'tv'} size={14} />
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
											{#if entry.rewatch}<span class="pill tiny">Rewatch</span>{/if}
											{#if entry.imported}<span class="pill tiny">Imported</span>{/if}
										</span>
										<span class="faint sub">
											{#if entry.film}
												Film
											{:else if entry.episodes.length === 1}
												{range(entry.episodes)}{#if entry.episodes[0].name}
													· {entry.episodes[0].name}{/if}
											{:else}
												{range(entry.episodes)} · {plural(entry.count, 'episode')}
											{/if}
										</span>
									</button>

									{#if entry.film && entry.rating !== null}
										<span class="score">{formatScore(entry.rating, fmt)}</span>
									{:else if entry.episodes.length === 1 && entry.episodes[0].rating !== null}
										<span class="score">{formatScore(entry.episodes[0].rating, fmt)}</span>
									{/if}

									{#if expandable}
										<span class="chev" class:turned={open === key}>
											<Icon name="chevron-down" size={16} />
										</span>
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
										{#each entry.episodes as ep (`${ep.season_number}-${ep.episode_number}`)}
											<li>
												<span class="code faint"
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
			<p class="faint undated">
				{plural(undated, 'watch', 'watches')} without a date are counted, but not shown here.
			</p>
		{/if}
	{/if}
</div>

<style>
	/* A diary is read down, not across: on a wide screen a full-width row
	   puts the rating a metre away from the title it belongs to. */
	.page > :global(*) {
		max-width: 980px;
	}

	.month {
		margin-bottom: 34px;
	}

	.month h2 {
		position: sticky;
		top: calc(var(--header-h) + 4px);
		z-index: 5;
		padding: 8px 0;
		margin-bottom: 6px;
		background: var(--bg);
		font-size: 1rem;
	}

	.entries {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.entry {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}

	.day {
		width: 2.4ch;
		padding-top: 18px;
		text-align: right;
		font-size: 0.78rem;
		font-variant-numeric: tabular-nums;
		flex: none;
	}

	.card-row {
		flex: 1;
		min-width: 0;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		overflow: hidden;
	}

	.entry.open .card-row {
		border-color: var(--line-strong);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 10px 8px 12px;
	}

	.art {
		width: 32px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
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
		gap: 2px;
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

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.line strong {
		font-size: 0.9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.sub {
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pill.tiny {
		padding: 1px 7px;
		font-size: 0.64rem;
		flex: none;
	}

	.score {
		font-weight: 650;
		font-size: 0.84rem;
		flex: none;
	}

	.score.small {
		font-size: 0.78rem;
	}

	.chev {
		display: grid;
		color: var(--text-faint);
		transition: transform var(--fast) var(--ease);
	}

	.chev.turned {
		transform: rotate(180deg);
	}

	.icon-btn {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: 8px;
		border: 1px solid transparent;
		background: none;
		color: var(--text-faint);
		cursor: pointer;
		flex: none;
	}

	.icon-btn:hover {
		color: var(--text);
		border-color: var(--line);
	}

	.episodes {
		list-style: none;
		margin: 0;
		padding: 4px 12px 10px 56px;
		display: flex;
		flex-direction: column;
		gap: 3px;
		border-top: 1px solid var(--line);
		max-height: 340px;
		overflow-y: auto;
	}

	.episodes li {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 5px 0;
		font-size: 0.82rem;
	}

	.code {
		font-variant-numeric: tabular-nums;
		font-size: 0.74rem;
		flex: none;
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
		padding-bottom: 20px;
	}

	@media (max-width: 720px) {
		.episodes {
			padding-left: 16px;
		}
	}
</style>
