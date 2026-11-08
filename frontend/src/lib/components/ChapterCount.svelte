<script lang="ts">
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';
	import type { LibraryItem } from '#lib/types';
	import Icon from './Icon.svelte';

	/*
	 * How far into a manga: chapters read, a step either way or typed. Taps
	 * count up at once and are saved together a moment after the last one,
	 * so reading through ten chapters is ten taps and one request.
	 */

	interface Props {
		item: LibraryItem;
		onchange: (item: LibraryItem) => void;
	}

	let { item, onchange }: Props = $props();

	/** What the counter shows, ahead of the server while taps are waiting. */
	let count = $state(0);
	let pending = 0;
	$effect(() => {
		if (!pending) count = item.watched_episodes;
	});

	const total = $derived(item.total_episodes > 0 ? item.total_episodes : null);
	const filled = $derived(total ? Math.min(100, (count / total) * 100) : 0);

	let timer: ReturnType<typeof setTimeout> | undefined;

	function set(next: number) {
		const held = Math.max(0, Math.min(total ?? Number.MAX_SAFE_INTEGER, Math.round(next)));
		if (!Number.isFinite(held) || held === count) return;
		count = held;
		pending += 1;
		clearTimeout(timer);
		timer = setTimeout(save, 450);
	}

	async function save() {
		const sent = count;
		try {
			const { item: updated } = await api.track(item.id, { progress: sent });
			pending = 0;
			onchange(updated);
		} catch (e) {
			pending = 0;
			count = item.watched_episodes;
			toasts.error(e);
		}
	}
</script>

<div class="chapters">
	{#if total}
		<span class="bar" aria-hidden="true"><i style:width="{filled}%"></i></span>
	{/if}
	<div class="row">
		<button
			class="btn icon step"
			aria-label="One chapter fewer"
			disabled={count <= 0}
			onclick={() => set(count - 1)}
		>
			<Icon name="minus" size={16} />
		</button>
		<label class="count">
			<span class="faint">Chapter</span>
			<input
				type="number"
				inputmode="numeric"
				min="0"
				max={total ?? undefined}
				value={count}
				aria-label="Chapters read"
				onchange={(e) => set(Number(e.currentTarget.value))}
				onkeydown={(e) => {
					if (e.key === 'Enter') e.currentTarget.blur();
				}}
			/>
			{#if total}<span class="faint">of {total}</span>{/if}
		</label>
		<button
			class="btn icon step"
			aria-label="One more chapter"
			disabled={total !== null && count >= total}
			onclick={() => set(count + 1)}
		>
			<Icon name="plus" size={16} />
		</button>
	</div>
</div>

<style>
	.chapters {
		display: flex;
		flex-direction: column;
		gap: 10px;
		max-width: 420px;
	}

	.bar {
		display: block;
		height: 4px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--text) 14%, transparent);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 260ms var(--ease);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.count {
		display: inline-flex;
		align-items: baseline;
		gap: 6px;
		font-size: 0.9rem;
		font-variant-numeric: tabular-nums;
	}

	.count input {
		width: 4.5em;
		padding: 4px 6px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		color: var(--text);
		font: inherit;
		font-weight: 700;
		text-align: center;
		-moz-appearance: textfield;
		appearance: textfield;
	}

	.count input::-webkit-inner-spin-button,
	.count input::-webkit-outer-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	@media (prefers-reduced-motion: reduce) {
		.bar i {
			transition: none;
		}
	}
</style>
