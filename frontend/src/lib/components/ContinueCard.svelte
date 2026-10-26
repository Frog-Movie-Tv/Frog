<script lang="ts">
	import Icon from './Icon.svelte';
	import { img } from '#lib/api';
	import { episodeCode, prettyDate } from '#lib/format';
	import type { UpNextEntry } from '#lib/types';

	interface Props {
		entry: UpNextEntry;
	}

	let { entry }: Props = $props();

	const item = $derived(entry.item);
	const next = $derived(entry.next);
	const backdrop = $derived(img(item.backdrop_path ?? item.poster_path, 'w780'));
	const progress = $derived(
		item.total_episodes > 0 ? (item.watched_episodes / item.total_episodes) * 100 : 0
	);
</script>

<a class="card" href="/title/{item.kind}/{item.tmdb_id}" style:--tint={item.accent.shade}>
	<div class="art">
		{#if backdrop}
			<img src={backdrop} alt="" loading="lazy" decoding="async" />
		{/if}
		<div class="veil" aria-hidden="true"></div>

		{#if item.logo_path}
			<img class="logo" src={img(item.logo_path, 'w300')} alt={item.name} loading="lazy" />
		{:else}
			<span class="name">{item.name}</span>
		{/if}

		{#if next?.aired !== false}
			<span class="play" aria-hidden="true"><Icon name="play" size={13} filled /></span>
		{:else}
			<span class="waiting"><Icon name="clock" size={13} /> Caught up</span>
		{/if}
	</div>

	<div class="meta">
		{#if next && next.aired && item.watched_episodes === 0}
			<span class="next">
				<strong>Start</strong>
				<span class="ep"
					>{episodeCode(next.season_number, next.episode_number)}{next.name
						? ` · ${next.name}`
						: ''}</span
				>
			</span>
		{:else if next && next.aired}
			<span class="next">
				<strong>{episodeCode(next.season_number, next.episode_number)}</strong>
				{#if next.name}<span class="ep">{next.name}</span>{/if}
			</span>
		{:else if next}
			<span class="next muted">
				Next episode {next.air_date ? prettyDate(next.air_date) : 'to be announced'}
			</span>
		{:else}
			<span class="next muted">Finished</span>
		{/if}

		{#if item.total_episodes > 0}
			<div class="bar" aria-label="{item.watched_episodes} of {item.total_episodes} episodes">
				<i style:width="{progress}%"></i>
			</div>
			<span class="faint small">{item.watched_episodes} / {item.total_episodes} episodes</span>
		{/if}
	</div>
</a>

<style>
	.card {
		display: flex;
		flex-direction: column;
		min-width: 0;
		border-radius: var(--radius);
		overflow: hidden;
		background: var(--bg-elev);
		border: 1px solid var(--line);
		transition:
			transform var(--slow) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.card:hover,
	.card:focus-visible {
		transform: translateY(-3px);
		border-color: var(--line-strong);
	}

	/* 16:9, the shape a streaming app uses for something you are mid-way
	   through -- a full-width portrait poster is a thousand pixels tall. */
	.art {
		position: relative;
		aspect-ratio: 16 / 9;
		background: var(--tint, var(--bg-elev-2));
		overflow: hidden;
	}

	.art > img:first-child {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.veil {
		position: absolute;
		inset: 0;
		background: linear-gradient(to top, rgba(0, 0, 0, 0.82), rgba(0, 0, 0, 0.1) 60%);
	}

	.logo {
		position: absolute;
		left: 14px;
		bottom: 12px;
		max-width: 58%;
		max-height: 42px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 2px 10px rgba(0, 0, 0, 0.7));
	}

	.name {
		position: absolute;
		left: 14px;
		bottom: 12px;
		right: 50px;
		font-weight: 650;
		font-size: 1rem;
		color: #fff;
		text-shadow: 0 2px 10px rgba(0, 0, 0, 0.7);
		line-height: 1.2;
	}

	.waiting {
		position: absolute;
		right: 12px;
		bottom: 12px;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 11px;
		border-radius: var(--radius-pill);
		background: rgba(10, 12, 11, 0.74);
		border: 1px solid rgba(255, 255, 255, 0.16);
		color: #fff;
		font-size: 0.72rem;
		font-weight: 600;
	}

	.play {
		position: absolute;
		right: 12px;
		bottom: 12px;
		width: 34px;
		height: 34px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		background: var(--accent);
		color: var(--accent-ink);
		box-shadow: var(--shadow-2);
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 12px 14px 14px;
	}

	.next {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
		font-size: 0.86rem;
	}

	.next strong {
		font-variant-numeric: tabular-nums;
		color: var(--accent);
		flex: none;
	}

	.ep {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.bar {
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.small {
		font-size: 0.72rem;
	}
</style>
