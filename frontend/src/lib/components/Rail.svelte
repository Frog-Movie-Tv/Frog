<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		action?: Snippet;
		children: Snippet;
	}

	let { title, action, children }: Props = $props();
</script>

<section class="rail">
	{#if title || action}
		<div class="section-head">
			{#if title}<h2>{title}</h2>{/if}
			{#if action}{@render action()}{/if}
		</div>
	{/if}
	<div class="track">{@render children()}</div>
</section>

<style>
	.rail {
		margin-bottom: 40px;
	}

	/* Horizontal scroll with snap, and the page gutter bled back in so cards
	   run to the edge of the screen on a phone. */
	.track {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(var(--poster-min), 1fr);
		gap: var(--gap);
		overflow-x: auto;
		scroll-snap-type: x proximity;
		padding: 2px var(--page-pad) 14px;
		margin: -2px calc(var(--page-pad) * -1) -14px;
		scrollbar-width: none;
	}

	.track::-webkit-scrollbar {
		display: none;
	}

	.track > :global(*) {
		scroll-snap-align: start;
	}

	@media (min-width: 900px) {
		.track {
			grid-auto-columns: minmax(var(--poster-min), 180px);
		}
	}
</style>
