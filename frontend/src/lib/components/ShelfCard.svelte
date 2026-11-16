<script lang="ts">
	import { img } from '#lib/api';
	import { plural } from '#lib/format';
	import Icon from './Icon.svelte';
	import { contextMenu, type MenuSource } from '#lib/menu.svelte';
	import { morph } from '#lib/morph.svelte';
	import type { Shelf } from '#lib/types';
	import type { Snippet } from 'svelte';

	interface Props {
		shelf: Pick<Shelf, 'id' | 'name' | 'count' | 'posters'> & { kind?: Shelf['kind'] };
		/** False while the shelves are being arranged: the card is not a link. */
		link?: boolean;
		/** What a right-click, or a hold, offers. */
		menu?: MenuSource;
		/** Its place on the page, so the cards arrive one after another. */
		index?: number;
		children?: Snippet;
	}

	let { shelf, link = true, menu, index = 0, children }: Props = $props();

	// The shelf opened carries its name into the shelf's heading, and back.
	const key = $derived(`shelf${shelf.id}`);
	const named = $derived(link && morph.key === key);

	/** Remember this shelf as the one opened, when it is a link. */
	function opens(node: HTMLElement, opened: string | null) {
		let current = opened;
		const click = () => {
			if (current) morph.key = current;
		};
		node.addEventListener('click', click);
		return {
			update(next: string | null) {
				current = next;
			},
			destroy() {
				node.removeEventListener('click', click);
			}
		};
	}

	/** Three posters fanned out, the first in front. */
	const fan = $derived(shelf.posters.slice(0, 3));
</script>

<svelte:element
	this={link ? 'a' : 'div'}
	class="shelf"
	href={link ? `/shelves/${shelf.id}` : undefined}
	use:contextMenu={link ? menu : undefined}
	style:--i={Math.min(index, 12)}
	use:opens={link ? key : null}
>
	<span class="cover" aria-hidden="true">
		{#if fan.length}
			{#each fan as poster, i (i)}
				<img class="p{i}" src={img(poster, 'w185')} alt="" loading="lazy" />
			{/each}
		{:else}
			<span class="blank"><Icon name="layers" size={26} /></span>
		{/if}
	</span>
	<span class="meta">
		<strong style:view-transition-name={named ? `shelf-name-${shelf.id}` : undefined}
			>{shelf.name}</strong
		>
		<span class="faint">
			{#if shelf.kind === 'smart'}<Icon name="sparkle" size={12} /> Smart ·{:else if shelf.kind === 'watchlist'}<Icon
					name="bookmark"
					size={12}
				/> Watchlist ·{/if}
			{plural(shelf.count, 'title')}
		</span>
	</span>
	{@render children?.()}
</svelte:element>

<style>
	/* The cards arrive one after another, and each deals its posters out. */
	.shelf {
		--step: calc(var(--i, 0) * 45ms);
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 12px;
		color: inherit;
		text-decoration: none;
		animation: arrive 520ms var(--spring) var(--step) backwards;
	}

	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(14px) scale(0.97);
		}
	}

	.cover {
		position: relative;
		display: block;
		aspect-ratio: 16 / 11;
		border-radius: var(--radius);
		background: var(--bg-elev);
		overflow: hidden;
		transition:
			background var(--fast) var(--ease),
			transform 320ms var(--spring),
			box-shadow 320ms var(--ease);
	}

	/* A light from under the posters, and a sheen across the glass. */
	.cover::before,
	.cover::after {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
		opacity: 0;
		transition: opacity 320ms var(--ease);
	}

	.cover::before {
		z-index: 0;
		background: radial-gradient(70% 60% at 50% 105%, var(--accent-soft), transparent 70%);
	}

	.cover::after {
		z-index: 4;
		background: linear-gradient(
			115deg,
			transparent 30%,
			rgb(255 255 255 / 0.12) 45%,
			transparent 60%
		);
		transform: translateX(-60%);
		transition:
			opacity 320ms var(--ease),
			transform 700ms var(--ease);
	}

	a.shelf:hover .cover {
		background: var(--bg-hover);
		transform: translateY(-3px);
		box-shadow: var(--shadow-2);
	}

	a.shelf:hover .cover::before,
	a.shelf:hover .cover::after {
		opacity: 1;
	}

	a.shelf:hover .cover::after {
		transform: translateX(60%);
	}

	a.shelf:active .cover {
		transform: scale(0.98);
		transition-duration: 120ms;
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
		transition: transform 420ms var(--spring);
	}

	.p0 {
		z-index: 3;
		transform: translateX(-50%);
		animation: rise-front 560ms var(--spring) calc(var(--step) + 80ms) backwards;
	}

	.p1 {
		z-index: 2;
		transform: translateX(-108%) rotate(-7deg) scale(0.9);
		transform-origin: bottom center;
		animation: deal-left 640ms var(--spring) calc(var(--step) + 200ms) backwards;
	}

	.p2 {
		z-index: 1;
		transform: translateX(8%) rotate(7deg) scale(0.9);
		transform-origin: bottom center;
		animation: deal-right 640ms var(--spring) calc(var(--step) + 260ms) backwards;
	}

	/* Dealt from behind the front poster. */
	@keyframes rise-front {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(18%);
		}
	}

	@keyframes deal-left {
		from {
			transform: translateX(-50%) rotate(0) scale(0.9);
		}
	}

	@keyframes deal-right {
		from {
			transform: translateX(-50%) rotate(0) scale(0.9);
		}
	}

	a.shelf:hover .p0 {
		transform: translateX(-50%) translateY(-8px) scale(1.04);
	}

	a.shelf:hover .p1 {
		transform: translateX(-120%) translateY(-2px) rotate(-11deg) scale(0.9);
	}

	a.shelf:hover .p2 {
		transform: translateX(20%) translateY(-2px) rotate(11deg) scale(0.9);
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
		transition: color var(--fast) var(--ease);
	}

	a.shelf:hover .meta strong {
		color: var(--accent);
	}

	.meta .faint {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 0.8rem;
	}
</style>
