<script lang="ts">
	import { clockParts, pad } from '#lib/events';

	interface Props {
		/** The moment it counts down to. */
		zero: string;
		/** Smaller tiles, for a page header beside other things. */
		compact?: boolean;
		/** Reached while this was on screen. */
		onzero?: () => void;
	}

	let { zero, compact = false, onzero }: Props = $props();

	const target = $derived(Date.parse(zero));
	let now = $state(Date.now());
	let told = false;

	// One tick a second, lined up with the clock's own second so the digits
	// turn together with the system clock beside them.
	$effect(() => {
		let timer: ReturnType<typeof setTimeout>;
		const tick = () => {
			now = Date.now();
			if (now >= target && !told) {
				told = true;
				onzero?.();
			}
			timer = setTimeout(tick, 1000 - (now % 1000) + 5);
		};
		tick();
		return () => clearTimeout(timer);
	});

	const parts = $derived(clockParts(target - now));
	const tiles = $derived([
		{ n: parts.days, unit: parts.days === 1 ? 'day' : 'days' },
		{ n: parts.hours, unit: 'hours' },
		{ n: parts.minutes, unit: 'min' },
		{ n: parts.seconds, unit: 'sec' }
	]);
	/** Read out to the minute: a voice reading every second is noise. */
	const spoken = $derived(
		`${parts.days} ${parts.days === 1 ? 'day' : 'days'}, ${parts.hours} hours and ${parts.minutes} minutes to go`
	);
</script>

<div class="countdown" class:compact role="timer" aria-label={spoken}>
	{#each tiles as t, i (t.unit)}
		<div class="tile" aria-hidden="true">
			<span class="n">{i === 0 ? t.n : pad(t.n)}</span>
			<span class="unit">{t.unit}</span>
		</div>
	{/each}
</div>

<style>
	.countdown {
		display: flex;
		gap: 10px;
		overflow-wrap: normal;
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-width: 76px;
		padding: 12px 10px 10px;
		border: 1px solid color-mix(in srgb, var(--text) 16%, transparent);
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--bg) 42%, transparent);
		backdrop-filter: var(--frost);
		box-shadow: inset 0 1px 0 color-mix(in srgb, var(--text) 10%, transparent);
	}

	.n {
		font-size: 2.3rem;
		font-weight: 800;
		line-height: 1;
		letter-spacing: -0.03em;
		font-variant-numeric: tabular-nums;
		color: var(--text);
	}

	.unit {
		margin-top: 6px;
		font-size: 0.68rem;
		font-weight: 700;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--text-dim);
	}

	.compact .tile {
		min-width: 60px;
		padding: 9px 8px 8px;
	}

	.compact .n {
		font-size: 1.7rem;
	}

	@media (max-width: 480px) {
		.countdown {
			gap: 6px;
		}

		.tile {
			min-width: 0;
			flex: 1 1 0;
			padding: 10px 4px 8px;
		}

		.n {
			font-size: 1.8rem;
		}
	}
</style>
