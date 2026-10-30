<script lang="ts">
	import { formatScore, fromDisplay, scale, toDisplay } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import Icon from './Icon.svelte';

	interface Props {
		/** 0..100, or null for unrated. */
		value: number | null;
		onchange: (value: number | null) => void;
		/** Smaller type and tighter padding, for episode rows. */
		compact?: boolean;
		disabled?: boolean;
	}

	let { value, onchange, compact = false, disabled = false }: Props = $props();

	let open = $state(false);
	let root: HTMLElement | undefined = $state();

	const format = $derived(settings.current.scoreFormat);
	const bounds = $derived(scale(format));
	const shown = $derived(toDisplay(value, format));
	const label = $derived(value === null ? 'Rate' : formatScore(value, format));

	/**
	 * The choices offered in the popover, in display units. A hundred buttons
	 * would be absurd, so the fine-grained formats step in fives and halves:
	 * about twenty targets, which is one comfortable tap each.
	 */
	const steps = $derived.by(() => {
		let step = bounds.step;
		if (format === 'pt100') step = 5;
		else if (format === 'pt10dec') step = 0.5;
		else if (format === 'star5') step = settings.current.halfStars ? 0.5 : 1;

		const out: number[] = [];
		for (let v = step; v <= bounds.max + 1e-9; v += step) out.push(Math.round(v * 10) / 10);
		return out;
	});

	function choose(display: number) {
		const same = shown !== null && Math.abs(shown - display) < 1e-6;
		onchange(same ? null : fromDisplay(display, format));
		open = false;
	}

	function onWindowClick(event: MouseEvent) {
		if (open && root && !root.contains(event.target as Node)) open = false;
	}

	function onKey(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			open = false;
			event.stopPropagation();
		}
	}
</script>

<svelte:window onclick={onWindowClick} onkeydown={onKey} />

<div class="rating" class:compact bind:this={root}>
	<button
		class="chip hit"
		class:rated={value !== null}
		{disabled}
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label={value === null ? 'Rate this' : `Rated ${label}`}
		title={value === null ? 'Rate this' : `Rated ${label}`}
		onclick={() => (open = !open)}
	>
		{#if format === 'star5' && value !== null}
			<span class="stars" aria-hidden="true">
				{#each [1, 2, 3, 4, 5] as s (s)}
					<i
						class:on={shown !== null && shown >= s - 0.5}
						class:half={shown !== null && shown >= s - 0.5 && shown < s}>★</i
					>
				{/each}
			</span>
			<span class="sr">{label}</span>
		{:else}
			{#if !compact}<Icon name="star" size={15} filled={value !== null} />{/if}
			<span class="value">{label}</span>
		{/if}
	</button>

	{#if open}
		<div class="pop" role="menu" aria-label="Choose a rating">
			<div class="grid" data-format={format}>
				{#each steps as step (step)}
					<button
						class="opt"
						class:on={shown !== null && Math.abs(shown - step) < 1e-6}
						role="menuitemradio"
						aria-checked={shown !== null && Math.abs(shown - step) < 1e-6}
						onclick={() => choose(step)}
					>
						{formatScore(fromDisplay(step, format), format)}
					</button>
				{/each}
			</div>
			{#if value !== null}
				<button
					class="clear"
					onclick={() => {
						onchange(null);
						open = false;
					}}>Clear rating</button
				>
			{/if}
		</div>
	{/if}
</div>

<style>
	.rating {
		position: relative;
		display: inline-flex;
	}

	/* One small control that says what it is when empty, instead of a dash. */
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		min-width: 62px;
		justify-content: center;
		border-radius: var(--radius-pill);
		border: 1px dashed var(--line-strong);
		background: transparent;
		color: var(--text-dim);
		font-size: 0.84rem;
		font-weight: 650;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.chip:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--text);
	}

	.chip.rated {
		border-style: solid;
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
		background: var(--accent-soft);
		color: var(--text);
	}

	.chip:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.compact .chip {
		padding: 4px 10px;
		min-width: 50px;
		font-size: 0.76rem;
	}

	.value {
		font-variant-numeric: tabular-nums;
	}

	.stars {
		display: inline-flex;
		gap: 1px;
		font-style: normal;
	}

	.stars i {
		font-style: normal;
		color: var(--line-strong);
		font-size: 0.86em;
	}

	.stars i.on {
		color: var(--accent);
	}

	.stars i.half {
		background: linear-gradient(90deg, var(--accent) 50%, var(--line-strong) 50%);
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}

	.pop {
		position: absolute;
		z-index: 60;
		top: calc(100% + 6px);
		right: 0;
		padding: 10px;
		border-radius: var(--radius);
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
		min-width: max-content;
	}

	.grid {
		display: grid;
		gap: 4px;
		grid-template-columns: repeat(5, minmax(34px, 1fr));
	}

	.grid[data-format='pt100'],
	.grid[data-format='pt10dec'] {
		grid-template-columns: repeat(5, minmax(42px, 1fr));
	}

	.grid[data-format='smiley3'] {
		grid-template-columns: repeat(3, 1fr);
	}

	.opt {
		padding: 7px 4px;
		border-radius: var(--radius-sm);
		border: 1px solid transparent;
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
		white-space: nowrap;
	}

	.opt:hover {
		background: var(--bg-hover);
		color: var(--text);
	}

	.opt.on {
		background: var(--accent);
		border-color: transparent;
		color: var(--accent-ink);
	}

	.clear {
		margin-top: 8px;
		width: 100%;
		padding: 7px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: transparent;
		color: var(--text-faint);
		font-size: 0.78rem;
		cursor: pointer;
	}

	.clear:hover {
		color: var(--text);
		border-color: var(--line-strong);
	}
</style>
