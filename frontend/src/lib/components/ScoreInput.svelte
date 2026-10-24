<script lang="ts">
	import { formatScore, fromDisplay, scale, toDisplay } from '#lib/format';
	import { settings } from '#lib/settings.svelte';

	interface Props {
		/** 0..100, or null for unrated. */
		value: number | null;
		onchange: (value: number | null) => void;
		compact?: boolean;
	}

	let { value, onchange, compact = false }: Props = $props();

	const format = $derived(settings.current.scoreFormat);
	const bounds = $derived(scale(format));
	const shown = $derived(toDisplay(value, format));

	function pick(display: number) {
		// Clicking the value you already have clears it: one control, both jobs.
		const same = shown !== null && Math.abs(shown - display) < bounds.step / 2;
		onchange(same ? null : fromDisplay(display, format));
	}

	const stars = [1, 2, 3, 4, 5];
	const smileys = [
		{ v: 1, face: '🙁', label: 'Did not like it' },
		{ v: 2, face: '😐', label: 'It was fine' },
		{ v: 3, face: '🙂', label: 'Liked it' }
	];
</script>

{#if format === 'star5'}
	<div class="stars" class:compact role="group" aria-label="Rating">
		{#each stars as s (s)}
			<button
				type="button"
				class="star"
				class:on={shown !== null && shown >= s - 0.5}
				class:half={shown !== null && shown >= s - 0.5 && shown < s}
				aria-label="{s} stars"
				onclick={(e) => {
					// Left half of the star is the half-point.
					const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
					const half = e.clientX - box.left < box.width / 2;
					pick(half ? s - 0.5 : s);
				}}
			>
				★
			</button>
		{/each}
		{#if shown !== null}
			<button type="button" class="clear" onclick={() => onchange(null)} aria-label="Clear rating">
				×
			</button>
		{/if}
	</div>
{:else if format === 'smiley3'}
	<div class="smileys" class:compact role="group" aria-label="Rating">
		{#each smileys as s (s.v)}
			<button
				type="button"
				class="smiley"
				class:on={shown === s.v}
				title={s.label}
				aria-label={s.label}
				onclick={() => pick(s.v)}
			>
				{s.face}
			</button>
		{/each}
	</div>
{:else}
	<div class="numeric" class:compact>
		<input
			type="range"
			min="0"
			max={bounds.max}
			step={bounds.step}
			value={shown ?? 0}
			aria-label="Rating"
			oninput={(e) => onchange(fromDisplay(Number(e.currentTarget.value), format))}
		/>
		<output class="value">{formatScore(value, format)}</output>
		{#if value !== null}
			<button type="button" class="clear" onclick={() => onchange(null)} aria-label="Clear rating">
				×
			</button>
		{/if}
	</div>
{/if}

<style>
	.stars,
	.smileys,
	.numeric {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.star,
	.smiley,
	.clear {
		background: none;
		border: none;
		cursor: pointer;
		padding: 2px;
		line-height: 1;
		transition: transform var(--fast) var(--ease);
	}

	.star {
		font-size: 1.5rem;
		color: var(--line-strong);
	}

	.star.on {
		color: var(--accent);
	}

	/* A half star is drawn by clipping the glyph, which keeps one element. */
	.star.half {
		background: linear-gradient(90deg, var(--accent) 50%, var(--line-strong) 50%);
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
	}

	.star:hover,
	.smiley:hover {
		transform: scale(1.14);
	}

	.smiley {
		font-size: 1.4rem;
		filter: grayscale(1) opacity(0.45);
	}

	.smiley.on {
		filter: none;
	}

	.numeric {
		gap: 12px;
		width: 100%;
	}

	input[type='range'] {
		flex: 1;
		min-width: 120px;
		accent-color: var(--accent);
		height: 4px;
	}

	.value {
		min-width: 3.2ch;
		text-align: right;
		font-variant-numeric: tabular-nums;
		font-weight: 700;
		font-size: 1rem;
	}

	.clear {
		font-size: 1.1rem;
		color: var(--text-faint);
	}

	.clear:hover {
		color: var(--text);
	}

	.compact .value {
		font-size: 0.88rem;
	}
	.compact .star {
		font-size: 1.15rem;
	}
	.compact .smiley {
		font-size: 1.05rem;
	}
</style>
