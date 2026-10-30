<script lang="ts">
	import Icon from './Icon.svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		/** What it filters, shown when nothing is chosen. */
		label: string;
		/** What is chosen, in a few words; null when nothing is. */
		value?: string | null;
		/** A wider panel, for a grid of choices. */
		wide?: boolean;
		children: Snippet;
	}

	let { label, value = null, wide = false, children }: Props = $props();

	let open = $state(false);
	let root: HTMLDivElement | undefined = $state();
</script>

<svelte:window
	onclick={(e) => {
		if (open && root && !root.contains(e.target as Node)) open = false;
	}}
	onkeydown={(e) => {
		if (open && e.key === 'Escape') open = false;
	}}
/>

<div class="filter" bind:this={root}>
	<button
		type="button"
		class="pill"
		class:set={value !== null}
		aria-haspopup="dialog"
		aria-expanded={open}
		onclick={() => (open = !open)}
	>
		<span class="name">{label}</span>
		{#if value !== null}<span class="value">{value}</span>{/if}
		<Icon name="chevron-down" size={15} />
	</button>

	{#if open}
		<div class="panel" class:wide role="dialog" aria-label={label}>
			{@render children()}
		</div>
	{/if}
</div>

<style>
	.filter {
		position: relative;
		flex: none;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 36px;
		padding: 0 12px 0 14px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.86rem;
		font-weight: 600;
		white-space: nowrap;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.pill:hover,
	.pill[aria-expanded='true'] {
		border-color: var(--line-strong);
		color: var(--text);
	}

	/* Chosen: the accent says this one is narrowing the results. */
	.pill.set {
		border-color: var(--accent);
		background: var(--accent-soft);
		color: var(--text);
	}

	.value {
		max-width: 160px;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--accent);
	}

	.pill.set .name::after {
		content: ':';
	}

	.panel {
		position: absolute;
		z-index: 60;
		top: calc(100% + 8px);
		left: 0;
		width: max-content;
		min-width: 220px;
		max-width: min(320px, calc(100vw - 32px));
		padding: 12px;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius);
		background: var(--bg-elev);
		box-shadow: var(--shadow-3);
	}

	.panel.wide {
		max-width: min(440px, calc(100vw - 32px));
	}

	@media (max-width: 760px) {
		.pill {
			height: 40px;
		}

		/* On a phone the panel opens as a sheet along the bottom, where a
		   thumb reaches it and the row it came from can scroll freely. */
		.panel,
		.panel.wide {
			position: fixed;
			top: auto;
			left: 12px;
			right: 12px;
			bottom: calc(80px + env(safe-area-inset-bottom));
			width: auto;
			max-width: none;
			max-height: 60vh;
			overflow-y: auto;
		}
	}
</style>
