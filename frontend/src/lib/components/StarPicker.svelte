<script lang="ts">
	import { STARS, starStep, starsAt } from '#lib/score-entry';
	import { ICONS } from './Icon.svelte';

	/*
	 * Stars filled in to rate. The row starts empty; a hand over it fills it up
	 * to where it is, and letting go gives that many. Left of the first star is
	 * zero stars, a rating of its own. The left half of a star
	 * is half of it, when halves are on. A finger can slide along the row before
	 * it lets go, and the arrow keys move it a step at a time, Enter to give it.
	 * Where a touch lands is `starsAt`, tested in `tests/score-entry.test.mjs`.
	 */

	interface Props {
		/** Stars given so far, out of five, or null for none. */
		value?: number | null;
		halves: boolean;
		/** One star's width, in pixels. */
		size?: number;
		/** Focus the row when it appears, on a device with a keyboard. */
		autofocus?: boolean;
		/** Anything that changes here empties what the hand was showing, as for the next card. */
		reset?: unknown;
		onpick: (stars: number) => void;
	}

	let { value = null, halves, size = 36, autofocus = false, reset, onpick }: Props = $props();

	/** What the row shows while a hand or the keys move over it. */
	let preview = $state<number | null>(null);
	let held = $state(false);
	let row: HTMLElement | undefined = $state();
	/** The stars themselves, without the zero before them. */
	let stars: HTMLElement | undefined = $state();

	const given = $derived(preview ?? value);
	const shown = $derived(given ?? 0);
	const words = $derived(given !== null ? `${starWords(shown)} of ${STARS}` : 'Not rated');

	$effect(() => {
		void reset;
		preview = null;
		held = false;
	});

	$effect(() => {
		if (autofocus && window.matchMedia('(hover: hover)').matches) row?.focus();
	});

	function starWords(n: number): string {
		const whole = Math.floor(n);
		const half = n - whole >= 0.5;
		if (!whole) return half ? '½' : '0';
		return `${whole}${half ? '½' : ''}`;
	}

	function at(e: PointerEvent): number {
		const box = stars?.getBoundingClientRect();
		if (!box || !box.width) return halves ? 0.5 : 1;
		return starsAt((e.clientX - box.left) / box.width, halves);
	}

	function down(e: PointerEvent) {
		if (e.button !== 0) return;
		held = true;
		row?.setPointerCapture(e.pointerId);
		preview = at(e);
	}

	function move(e: PointerEvent) {
		if (held || e.pointerType === 'mouse') preview = at(e);
	}

	function up(e: PointerEvent) {
		if (!held) return;
		held = false;
		const stars = at(e);
		// A mouse still over the row keeps showing where it is; a finger lifts.
		preview = e.pointerType === 'mouse' ? stars : null;
		onpick(stars);
	}

	function cancel() {
		held = false;
		preview = null;
	}

	function key(e: KeyboardEvent) {
		let next: number | null = null;
		if (e.key === 'ArrowRight' || e.key === 'ArrowUp') next = starStep(shown, 1, halves);
		else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') next = starStep(shown, -1, halves);
		else if (e.key === 'Home') next = 0;
		else if (e.key === 'End') next = STARS;
		else if ((e.key === 'Enter' || e.key === ' ') && given !== null) {
			e.preventDefault();
			e.stopPropagation();
			onpick(shown);
			return;
		}
		if (next === null) return;
		e.preventDefault();
		e.stopPropagation();
		preview = next;
	}

	/** How much of star `i` is filled, 0 to 1. */
	const fill = (i: number) => Math.min(1, Math.max(0, shown - i));
</script>

<div class="picker" style:--size="{size}px">
	<div
		bind:this={row}
		class="row"
		class:held
		role="slider"
		tabindex="0"
		aria-label="Stars"
		aria-valuemin={0}
		aria-valuemax={STARS}
		aria-valuenow={shown}
		aria-valuetext={words}
		onpointerdown={down}
		onpointermove={move}
		onpointerup={up}
		onpointercancel={cancel}
		onpointerleave={() => {
			if (!held) preview = null;
		}}
		onkeydown={key}
		onblur={() => {
			if (!held) preview = null;
		}}
	>
		<span class="zero" class:on={given === 0} aria-hidden="true">0</span>
		<span class="stars" bind:this={stars}>
			{#each { length: STARS }, i (i)}
				<span class="star" class:lit={fill(i) > 0}>
					<svg class="outline" viewBox="0 0 256 256" width={size} height={size} aria-hidden="true">
						<path d={ICONS.star[0]} fill="currentColor" />
					</svg>
					<span class="fill" style:width="{fill(i) * 100}%">
						<svg viewBox="0 0 256 256" width={size} height={size} aria-hidden="true">
							<path d={ICONS.star[1]} fill="currentColor" />
						</svg>
					</span>
				</span>
			{/each}
		</span>
	</div>
	<span class="words" aria-hidden="true">{words}</span>
</div>

<style>
	.picker {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}

	.row {
		display: flex;
		gap: calc(var(--size) / 6);
		padding: 4px 6px;
		border-radius: var(--radius);
		cursor: pointer;
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
		-webkit-tap-highlight-color: transparent;
	}

	.row:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.stars {
		display: flex;
		gap: calc(var(--size) / 6);
	}

	/* Zero stars: a mark before the first, lit when it is the rating. */
	.zero {
		display: grid;
		place-items: center;
		width: calc(var(--size) * 0.55);
		color: var(--text-faint);
		font-size: calc(var(--size) * 0.36);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
		opacity: 0.7;
	}

	.zero.on {
		color: var(--accent);
		opacity: 1;
	}

	.star {
		position: relative;
		display: block;
		width: var(--size);
		height: var(--size);
		line-height: 0;
		transition: transform 160ms cubic-bezier(0.3, 1.5, 0.5, 1);
	}

	.outline {
		color: var(--text-faint);
	}

	.lit .outline {
		color: var(--accent);
	}

	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		overflow: hidden;
		color: var(--accent);
		transition: width 120ms ease-out;
	}

	.fill svg {
		display: block;
		flex: none;
	}

	/* The stars under a finger grow a little, so it can see what it fills. */
	.held .lit {
		transform: scale(1.08);
	}

	.held .fill {
		transition: none;
	}

	.words {
		min-height: 1.2em;
		color: var(--text-dim);
		font-size: 0.82rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	@media (prefers-reduced-motion: reduce) {
		.star,
		.fill {
			transition: none;
		}
	}
</style>
