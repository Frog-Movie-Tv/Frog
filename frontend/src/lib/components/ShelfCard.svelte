<script lang="ts">
	import { img } from '#lib/api';
	import { plural } from '#lib/format';
	import Icon from './Icon.svelte';
	import type { Shelf } from '#lib/types';
	import type { Snippet } from 'svelte';

	interface Props {
		shelf: Pick<Shelf, 'id' | 'name' | 'count' | 'posters'> & { kind?: Shelf['kind'] };
		/** False while the shelves are being arranged: the card is not a link. */
		link?: boolean;
		children?: Snippet;
	}

	let { shelf, link = true, children }: Props = $props();

	/** Three posters fanned out, the first in front. */
	const fan = $derived(shelf.posters.slice(0, 3));
</script>

<svelte:element
	this={link ? 'a' : 'div'}
	class="shelf"
	href={link ? `/shelves/${shelf.id}` : undefined}
>
	<span class="cover" aria-hidden="true">
		{#if fan.length}
			{#each fan as poster, i (poster)}
				<img class="p{i}" src={img(poster, 'w185')} alt="" loading="lazy" />
			{/each}
		{:else}
			<span class="blank"><Icon name="layers" size={26} /></span>
		{/if}
	</span>
	<span class="meta">
		<strong>{shelf.name}</strong>
		<span class="faint">
			{#if shelf.kind === 'smart'}<Icon name="sparkle" size={12} /> Smart ·{/if}
			{plural(shelf.count, 'title')}
		</span>
	</span>
	{@render children?.()}
</svelte:element>

<style>
	.shelf {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: inherit;
		text-decoration: none;
	}

	.cover {
		position: relative;
		display: block;
		aspect-ratio: 16 / 11;
		border-radius: var(--radius);
		background: var(--bg-elev);
		overflow: hidden;
		transition: background var(--fast) var(--ease);
	}

	a.shelf:hover .cover {
		background: var(--bg-elev-2);
	}

	.cover img {
		position: absolute;
		bottom: -6%;
		left: 50%;
		width: 38%;
		aspect-ratio: 2 / 3;
		object-fit: cover;
		border-radius: 6px;
		box-shadow: 0 8px 20px -6px rgb(0 0 0 / 0.6);
		transition: transform var(--slow) var(--ease);
	}

	.p0 {
		z-index: 3;
		transform: translateX(-50%);
	}

	.p1 {
		z-index: 2;
		transform: translateX(-108%) rotate(-7deg) scale(0.9);
		transform-origin: bottom center;
	}

	.p2 {
		z-index: 1;
		transform: translateX(8%) rotate(7deg) scale(0.9);
		transform-origin: bottom center;
	}

	a.shelf:hover .p0 {
		transform: translateX(-50%) translateY(-4px);
	}

	a.shelf:hover .p1 {
		transform: translateX(-114%) rotate(-9deg) scale(0.9);
	}

	a.shelf:hover .p2 {
		transform: translateX(14%) rotate(9deg) scale(0.9);
	}

	.blank {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--text-faint);
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
		padding: 0 2px;
	}

	.meta strong {
		font-size: 0.95rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta .faint {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 0.8rem;
	}
</style>
