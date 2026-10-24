<script lang="ts">
	import Icon from './Icon.svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		action?: Snippet;
		/** Wider cards, for the 16:9 continue-watching tiles. */
		wide?: boolean;
		children: Snippet;
	}

	let { title, action, wide = false, children }: Props = $props();

	let track: HTMLDivElement | undefined = $state();
	let atStart = $state(true);
	let atEnd = $state(false);

	function measure() {
		if (!track) return;
		const max = track.scrollWidth - track.clientWidth;
		atStart = track.scrollLeft <= 2;
		atEnd = track.scrollLeft >= max - 2;
	}

	function nudge(direction: -1 | 1) {
		if (!track) return;
		track.scrollBy({ left: direction * track.clientWidth * 0.8, behavior: 'smooth' });
	}

	$effect(() => {
		measure();
	});
</script>

<section class="rail" class:wide>
	{#if title || action}
		<div class="section-head">
			{#if title}<h2>{title}</h2>{/if}
			<div class="tools">
				{#if action}{@render action()}{/if}
				<div class="arrows">
					<button
						class="arrow"
						disabled={atStart}
						aria-label="Scroll left"
						onclick={() => nudge(-1)}
					>
						<Icon name="chevron-left" size={17} />
					</button>
					<button class="arrow" disabled={atEnd} aria-label="Scroll right" onclick={() => nudge(1)}>
						<Icon name="chevron-right" size={17} />
					</button>
				</div>
			</div>
		</div>
	{/if}

	<div class="track" bind:this={track} onscroll={measure}>
		{@render children()}
	</div>
</section>

<style>
	.rail {
		margin-bottom: 40px;
	}

	.tools {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.arrows {
		display: none;
		gap: 6px;
	}

	.arrow {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 1.1rem;
		line-height: 1;
		cursor: pointer;
		transition:
			color var(--fast) var(--ease),
			opacity var(--fast) var(--ease);
	}

	.arrow:hover:not(:disabled) {
		color: var(--text);
	}

	.arrow:disabled {
		opacity: 0.3;
		cursor: default;
	}

	/* Horizontal scroll with snap. The page gutter is bled back in so cards
	   run to the edge of a phone screen, and scroll-padding keeps the first
	   card clear of that gutter instead of hiding under it. */
	.track {
		display: grid;
		grid-auto-flow: column;
		/* Fixed, never 1fr: a single card in a row would otherwise stretch to
		   the full width of the page and stand seven hundred pixels tall. */
		grid-auto-columns: var(--poster-min);
		justify-content: start;
		gap: var(--gap);
		overflow-x: auto;
		scroll-snap-type: x proximity;
		scroll-padding-inline: var(--page-pad);
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
			grid-auto-columns: clamp(var(--poster-min), var(--poster-min), 190px);
		}

		.arrows {
			display: flex;
		}
	}

	/* Wide cards (continue watching) need more room than a poster. */
	.rail.wide .track {
		grid-auto-columns: min(86vw, 340px);
	}

	@media (min-width: 900px) {
		.rail.wide .track {
			grid-auto-columns: 340px;
		}
	}
</style>
