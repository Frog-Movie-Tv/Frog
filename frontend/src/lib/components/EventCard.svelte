<script lang="ts">
	import { img } from '#lib/api';
	import { caption } from '#lib/events';
	import { titleHref } from '#lib/titles';
	import type { EventListItem } from '#lib/types';
	import Icon from './Icon.svelte';

	interface Props {
		entry: EventListItem;
		/** Its place in watch order, from 1. */
		n: number;
	}

	let { entry, n }: Props = $props();

	const item = $derived(entry.item);
	const s = $derived(entry.standing);
	const progress = $derived(!s.seen && s.watched > 0 && s.total > 0 ? s.watched / s.total : 0);
</script>

<a
	class="card"
	class:seen={s.seen}
	class:optional={!entry.essential}
	href={titleHref(item)}
	aria-label="{n}. {item.name}, {caption(entry)}"
>
	<span class="art">
		{#if item.poster_path}
			<img src={img(item.poster_path, 'w342')} alt="" loading="lazy" />
		{:else}
			<span class="noart"></span>
		{/if}
		<span class="n">{n}</span>
		{#if s.seen}
			<span class="tick"><Icon name="check" size={16} /></span>
		{/if}
		{#if progress}
			<span class="bar"><span style:width="{Math.round(progress * 100)}%"></span></span>
		{/if}
	</span>
	<strong class="name">{item.name}</strong>
	<span class="caption">{caption(entry)}</span>
</a>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: 6px;
		color: inherit;
		text-decoration: none;
		min-width: 0;
	}

	.art {
		position: relative;
		display: block;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius);
		overflow: hidden;
		background: var(--bg-elev-2);
		border: 1px solid var(--line);
		transition: transform var(--fast) var(--ease);
	}

	.card:hover .art {
		transform: translateY(-2px);
	}

	.optional .art {
		border: 1.5px dashed var(--line-strong);
	}

	.art img,
	.noart {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.seen .art img {
		opacity: 0.42;
		filter: saturate(0.5);
	}

	.n {
		position: absolute;
		top: 8px;
		left: 8px;
		display: grid;
		place-items: center;
		min-width: 26px;
		height: 26px;
		padding: 0 7px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--bg) 70%, transparent);
		backdrop-filter: var(--frost);
		font-size: 0.8rem;
		font-weight: 800;
		font-variant-numeric: tabular-nums;
	}

	.tick {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--text);
	}

	.tick :global(svg) {
		width: 38px;
		height: 38px;
		padding: 9px;
		border-radius: 50%;
		background: var(--accent);
		color: var(--accent-ink);
	}

	.bar {
		position: absolute;
		left: 8px;
		right: 8px;
		bottom: 8px;
		height: 4px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--bg) 60%, transparent);
		overflow: hidden;
	}

	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.name {
		font-size: 0.88rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}

	.caption {
		font-size: 0.78rem;
		color: var(--text-dim);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}

	.seen .name,
	.seen .caption {
		opacity: 0.6;
	}
</style>
