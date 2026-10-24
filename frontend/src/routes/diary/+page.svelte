<script lang="ts">
	import { img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { episodeCode, formatScore, plural, prettyDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';

	interface Entry {
		watched_at: string;
		rewatch: boolean;
		source: string;
		kind: 'movie' | 'tv';
		tmdb_id: number;
		title: string;
		poster_path: string | null;
		season_number: number | null;
		episode_number: number | null;
		episode_name: string | null;
		rating: number | null;
	}

	let entries = $state<Entry[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let loadingMore = $state(false);

	$effect(() => {
		if (!session.authenticated) return;
		void load(0);
	});

	async function load(offset: number) {
		if (offset > 0) loadingMore = true;
		try {
			const res = await fetch(`/api/diary?limit=100&offset=${offset}`, {
				credentials: 'same-origin'
			});
			if (!res.ok) throw new Error('Could not load your diary');
			const data = await res.json();
			entries = offset === 0 ? data.entries : [...entries, ...data.entries];
			total = data.total;
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = false;
			loadingMore = false;
		}
	}

	/** Group by month, which is how anyone reads back a year of watching. */
	const months = $derived.by(() => {
		const out: { label: string; key: string; entries: Entry[] }[] = [];
		for (const entry of entries) {
			const key = entry.watched_at.slice(0, 7);
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
			hint="Tick an episode or mark a film watched and it will show up here."
		>
			<a class="btn primary" href="/library">Open library</a>
		</Empty>
	{:else}
		{#each months as month (month.key)}
			<section class="month">
				<h2>{month.label}</h2>
				<ol class="entries">
					{#each month.entries as entry, i (`${entry.watched_at}-${entry.tmdb_id}-${i}`)}
						<li>
							<span class="day faint">{entry.watched_at.slice(8, 10)}</span>
							<a href="/title/{entry.kind}/{entry.tmdb_id}">
								{#if entry.poster_path}
									<img src={img(entry.poster_path, 'w92')} alt="" loading="lazy" />
								{:else}
									<span class="no-art"><Icon name="film" size={14} /></span>
								{/if}

								<span class="what">
									<span class="line">
										<strong>{entry.title}</strong>
										{#if entry.rewatch}<span class="pill tiny">Rewatch</span>{/if}
										{#if entry.source === 'import'}<span class="pill tiny">Imported</span>{/if}
									</span>
									<span class="faint">
										{#if entry.season_number !== null && entry.episode_number !== null}
											{episodeCode(entry.season_number, entry.episode_number)}
											{#if entry.episode_name}· {entry.episode_name}{/if}
										{:else}
											{prettyDate(entry.watched_at)}
										{/if}
									</span>
								</span>

								{#if entry.rating !== null}
									<span class="score">{formatScore(entry.rating, fmt)}</span>
								{/if}
							</a>
						</li>
					{/each}
				</ol>
			</section>
		{/each}

		{#if entries.length < total}
			<div class="more">
				<button class="btn" disabled={loadingMore} onclick={() => load(entries.length)}>
					{loadingMore ? 'Loading' : `Show more (${total - entries.length} left)`}
				</button>
			</div>
		{/if}
	{/if}
</div>

<style>
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
		gap: 4px;
	}

	.entries li {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.day {
		width: 2.4ch;
		text-align: right;
		font-size: 0.78rem;
		font-variant-numeric: tabular-nums;
		flex: none;
	}

	.entries a {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 13px;
		padding: 8px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.entries a:hover {
		border-color: var(--line-strong);
	}

	img,
	.no-art {
		width: 32px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		object-fit: cover;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-faint);
	}

	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.line strong {
		font-size: 0.88rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.what .faint {
		font-size: 0.75rem;
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

	.more {
		display: flex;
		justify-content: center;
		padding: 10px 0 30px;
	}
</style>
