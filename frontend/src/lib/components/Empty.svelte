<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		hint?: string;
		children?: Snippet;
	}

	let { title, hint, children }: Props = $props();
</script>

<div class="empty">
	<h3>{title}</h3>
	{#if hint}<p class="muted">{hint}</p>{/if}
	{#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		text-align: center;
		padding: 72px 20px;
		border: 1px dashed var(--line-strong);
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--bg-elev) 60%, transparent);
	}

	h3 {
		font-size: 1.05rem;
	}

	p {
		max-width: 46ch;
		font-size: 0.9rem;
	}

	.actions {
		margin-top: 8px;
		display: flex;
		gap: 10px;
	}
</style>
