<script lang="ts">
	import Icon from './Icon.svelte';

	/*
	 * Many titles at once from a list that is not your library: a studio's,
	 * and in time a person's. One row: how many, all or none, add, done. It
	 * sits where Library's edit bar sits and lifts the toasts above it.
	 */

	interface Props {
		count: number;
		/** How many could be ticked, under the filters. */
		selectable: number;
		busy?: boolean;
		onselectall: () => void;
		onadd: (e: MouseEvent) => void;
		ondone: () => void;
	}

	let { count, selectable, busy = false, onselectall, onadd, ondone }: Props = $props();

	let height = $state(0);
	const all = $derived(selectable > 0 && count === selectable);

	$effect(() => {
		const root = document.documentElement.style;
		root.setProperty('--toast-lift', `${height}px`);
		return () => root.removeProperty('--toast-lift');
	});

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape' && !document.querySelector('[role="menu"], [role="dialog"]')) ondone();
	}
</script>

<svelte:window onkeydown={onKey} />

<div class="bar" role="toolbar" aria-label="Choose titles" bind:clientHeight={height}>
	<strong class="count" aria-live="polite">{count} selected</strong>
	<button class="link" disabled={!selectable} onclick={onselectall}>
		{all ? 'Select none' : 'Select all'}
	</button>
	<button class="btn primary add" disabled={!count || busy} aria-haspopup="menu" onclick={onadd}>
		<Icon name="plus" size={15} />
		{count ? `Add ${count}` : 'Add'}
	</button>
	<button class="btn ghost" onclick={ondone}>Done</button>
</div>

<style>
	/* In front of the tab bar on a phone, as Library's edit bar is. */
	.bar {
		position: fixed;
		z-index: 45;
		left: 50%;
		bottom: calc(16px + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 12px;
		width: min(520px, calc(100vw - 32px));
		padding: 10px 12px;
		border-radius: var(--radius);
		border: 1px solid var(--line-strong);
		background: var(--bg-float);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
	}

	.count {
		font-size: 0.92rem;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.link {
		padding: 8px 4px;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		font-size: 0.86rem;
		font-weight: 600;
		white-space: nowrap;
		cursor: pointer;
	}

	.link:disabled {
		color: var(--text-faint);
		cursor: default;
	}

	.add {
		margin-left: auto;
		white-space: nowrap;
	}

	@media (max-width: 420px) {
		.bar {
			gap: 8px;
		}
	}
</style>
