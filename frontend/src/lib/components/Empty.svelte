<script lang="ts">
	import type { Snippet } from 'svelte';
	import FrogArt from './FrogArt.svelte';

	interface Props {
		title: string;
		hint?: string;
		/** Which frog to sit in the middle of it. */
		art?: 'search' | 'rest' | 'done' | 'none';
		children?: Snippet;
	}

	let { title, hint, art = 'rest', children }: Props = $props();
</script>

<div class="empty">
	{#if art !== 'none'}
		<FrogArt mood={art} size={88} />
	{/if}
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
		padding: 60px 20px 72px;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--bg-elev) 60%, transparent);
	}

	h3 {
		font-size: 1.15rem;
		margin-top: 6px;
	}

	p {
		max-width: 44ch;
		font-size: 0.92rem;
	}

	.actions {
		margin-top: 12px;
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 10px;
	}
</style>
