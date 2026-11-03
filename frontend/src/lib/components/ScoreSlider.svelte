<script lang="ts">
	import { formatScore, fromDisplay, toDisplay, type ScoreFormat } from '#lib/format';
	import { marks, slideScale, snap } from '#lib/score-entry';
	import Icon from './Icon.svelte';

	/*
	 * A score slid to, for the formats with too many values for buttons
	 * (`lib/score-entry.ts`): drag the track, or step with − and + or the arrow
	 * keys, then give it with the button or Enter. The slider only goes where
	 * the scale goes, so a score is always on it.
	 */

	interface Props {
		format: ScoreFormat;
		/** 0..100, where the slider starts; null starts it in the middle. */
		value?: number | null;
		/** The button's word. */
		action?: string;
		/** Focus the slider when it appears, on a device with a keyboard. */
		autofocus?: boolean;
		/** Anything that changes here puts the slider back, as for the next card. */
		reset?: unknown;
		onsubmit: (value: number) => void;
	}

	let {
		format,
		value = null,
		action = 'Rate',
		autofocus = false,
		reset,
		onsubmit
	}: Props = $props();

	const scale = $derived(slideScale(format));
	let shown = $state(0);
	let dragging = $state(false);
	let range: HTMLInputElement | undefined = $state();

	$effect(() => {
		void reset;
		shown = snap(value === null ? scale.max / 2 : (toDisplay(value, format) ?? 0), format);
	});

	$effect(() => {
		// A phone would show nothing for it; there the slider waits to be touched.
		if (autofocus && window.matchMedia('(hover: hover)').matches) range?.focus();
	});

	const label = $derived(formatScore(fromDisplay(shown, format), format));
	const fill = $derived((shown / scale.max) * 100);

	function nudge(by: number) {
		shown = snap(shown + by * scale.step, format);
		range?.focus();
	}

	function give() {
		onsubmit(fromDisplay(snap(shown, format), format) ?? 0);
	}
</script>

<div class="slider" class:dragging style:--fill="{fill}%">
	<div class="top">
		<output class="value">
			{#key label}<span class="num">{label}</span>{/key}
			<span class="of">/ {scale.max}</span>
		</output>
		<button class="btn primary give" onclick={give}>{action}</button>
	</div>

	<div class="track-row">
		<button class="nudge" aria-label="One step lower" onclick={() => nudge(-1)}>
			<Icon name="minus" size={16} />
		</button>
		<input
			bind:this={range}
			bind:value={shown}
			type="range"
			min="0"
			max={scale.max}
			step={scale.step}
			aria-label="Score out of {scale.max}"
			aria-valuetext={label}
			onpointerdown={() => (dragging = true)}
			onpointerup={() => (dragging = false)}
			onpointercancel={() => (dragging = false)}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					give();
				}
			}}
		/>
		<button class="nudge" aria-label="One step higher" onclick={() => nudge(1)}>
			<Icon name="plus" size={16} />
		</button>
	</div>

	<div class="marks" aria-hidden="true">
		{#each marks(format) as m (m)}
			<span style:left="{(m / scale.max) * 100}%">{m}</span>
		{/each}
	</div>
</div>

<style>
	/* The fill can glide to a new value, as when − or + is pressed, and
	   follows the hand without delay while it drags. */
	@property --fill {
		syntax: '<percentage>';
		inherits: true;
		initial-value: 50%;
	}

	.slider {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: min(100%, 300px);
		transition: --fill 260ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}

	.slider.dragging {
		transition: none;
	}

	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.value {
		display: flex;
		align-items: baseline;
		gap: 6px;
		font-variant-numeric: tabular-nums;
	}

	.num {
		display: inline-block;
		font-family: var(--font-display);
		font-size: 2.1rem;
		font-weight: 800;
		line-height: 1;
		color: var(--text);
		animation: tick 160ms cubic-bezier(0.3, 1.5, 0.5, 1);
	}

	@keyframes tick {
		from {
			transform: scale(1.12);
			color: var(--accent);
		}
	}

	.of {
		color: var(--text-faint);
		font-size: 0.95rem;
		font-weight: 600;
	}

	.give {
		flex: none;
		min-height: 44px;
	}

	.track-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.nudge {
		flex: none;
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: var(--bg-elev);
		color: var(--text-dim);
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.nudge:hover {
		background: var(--bg-hover);
		color: var(--text);
	}

	.nudge:active {
		transform: scale(0.9);
	}

	/* The track: filled up to the value in the accent, the rest quiet. */
	input[type='range'] {
		flex: 1;
		min-width: 0;
		height: 32px;
		margin: 0;
		background: transparent;
		appearance: none;
		-webkit-appearance: none;
		cursor: pointer;
		touch-action: none;
	}

	input[type='range']:focus-visible {
		outline: none;
	}

	input[type='range']::-webkit-slider-runnable-track {
		height: 10px;
		border-radius: 999px;
		background: linear-gradient(90deg, var(--accent) var(--fill), var(--bg-elev-2) var(--fill));
	}

	input[type='range']::-moz-range-track {
		height: 10px;
		border-radius: 999px;
		background: linear-gradient(90deg, var(--accent) var(--fill), var(--bg-elev-2) var(--fill));
	}

	input[type='range']::-webkit-slider-thumb {
		-webkit-appearance: none;
		width: 26px;
		height: 26px;
		margin-top: -8px;
		border: 3px solid var(--accent);
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
		transition: transform 160ms cubic-bezier(0.3, 1.5, 0.5, 1);
	}

	input[type='range']::-moz-range-thumb {
		width: 20px;
		height: 20px;
		border: 3px solid var(--accent);
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
		transition: transform 160ms cubic-bezier(0.3, 1.5, 0.5, 1);
	}

	/* The thumb grows under the hand, and when the keys move it. */
	.dragging input[type='range']::-webkit-slider-thumb,
	input[type='range']:focus-visible::-webkit-slider-thumb {
		transform: scale(1.2);
	}

	.dragging input[type='range']::-moz-range-thumb,
	input[type='range']:focus-visible::-moz-range-thumb {
		transform: scale(1.2);
	}

	/* The marks sit under the track, between the two step buttons. */
	.marks {
		position: relative;
		height: 14px;
		margin: -4px 59px 0;
		color: var(--text-faint);
		font-size: 0.7rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.marks span {
		position: absolute;
		transform: translateX(-50%);
		white-space: nowrap;
	}

	@media (prefers-reduced-motion: reduce) {
		.slider {
			transition: none;
		}

		.num {
			animation: none;
		}
	}
</style>
