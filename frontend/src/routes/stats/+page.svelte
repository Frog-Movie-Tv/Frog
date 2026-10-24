<script lang="ts">
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { formatScore, longRuntime } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { Stats, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	let stats = $state<Stats | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		if (!session.authenticated) return;
		void api
			.stats()
			.then((s) => (stats = s))
			.catch((e) => (error = e instanceof Error ? e.message : 'Could not load stats'))
			.finally(() => (loading = false));
	});

	const total = $derived(
		STATUSES.reduce((sum, s) => sum + (stats?.by_status[s as Status] ?? 0), 0)
	);
</script>

<svelte:head>
	<title>Stats · Frog</title>
</svelte:head>


<div class="page">
	<h1 class="title">Stats</h1>

	{#if loading}
		<Spinner label="Counting" />
	{:else if error || !stats}
		<Empty title="Could not load stats" hint={error ?? undefined} />
	{:else}
		<div class="tiles">
			<div class="tile card">
				<span class="eyebrow">Titles tracked</span>
				<strong>{total}</strong>
				<span class="faint">
					{stats.by_kind.movie ?? 0} films · {stats.by_kind.tv ?? 0} shows
				</span>
			</div>

			<div class="tile card">
				<span class="eyebrow">Films watched</span>
				<strong>{stats.films_watched}</strong>
			</div>

			<div class="tile card">
				<span class="eyebrow">Episodes watched</span>
				<strong>{stats.episodes_watched}</strong>
			</div>

			<div class="tile card">
				<span class="eyebrow">Time watched</span>
				<strong>{longRuntime(stats.minutes_watched)}</strong>
				<span class="faint">estimated from runtimes</span>
			</div>

			<div class="tile card">
				<span class="eyebrow">Mean rating</span>
				<strong>{formatScore(stats.mean_rating, settings.current.scoreFormat)}</strong>
			</div>
		</div>

		{#if total > 0}
			<section class="block">
				<div class="section-head"><h2>By status</h2></div>
				<div class="bar" role="img" aria-label="Library by status">
					{#each STATUSES as s (s)}
						{@const n = stats.by_status[s] ?? 0}
						{#if n > 0}
							<span
								class="seg"
								data-status={s}
								style:flex-grow={n}
								title="{STATUS_LABEL[s]}: {n}"
							></span>
						{/if}
					{/each}
				</div>
				<ul class="legend">
					{#each STATUSES as s (s)}
						{@const n = stats.by_status[s] ?? 0}
						{#if n > 0}
							<li>
								<span class="swatch" data-status={s}></span>
								{STATUS_LABEL[s]}
								<span class="faint">{n}</span>
							</li>
						{/if}
					{/each}
				</ul>
			</section>
		{/if}

		<p class="faint note">
			Phase 2 turns this into the full stats page, and phase 4 adds the shareable year in review.
		</p>
	{/if}
</div>

<style>
	.title {
		margin-bottom: 22px;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
		gap: 14px;
		margin-bottom: 44px;
	}

	.tile {
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.tile strong {
		font-size: 1.9rem;
		font-weight: 650;
		letter-spacing: -0.03em;
		line-height: 1.1;
	}

	.tile .faint {
		font-size: 0.76rem;
	}

	.block {
		margin-bottom: 36px;
	}

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
		background: var(--accent);
	}

	[data-status='completed'] {
		background: var(--good);
	}
	[data-status='planned'] {
		background: var(--text-faint);
	}
	[data-status='paused'] {
		background: var(--warn);
	}
	[data-status='dropped'] {
		background: var(--bad);
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

	.note {
		font-size: 0.78rem;
	}
</style>
