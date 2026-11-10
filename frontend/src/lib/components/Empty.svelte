<script lang="ts">
	import type { Snippet } from 'svelte';
	import Mascot, { type Pose } from './Mascot.svelte';

	/**
	 * What Pip does over an empty state. The first three are the old names,
	 * kept so every page reads the same: `rest` is nothing here yet,
	 * `search` is nothing found, `done` is all finished.
	 */
	type Art = 'search' | 'rest' | 'done' | 'lost' | 'failed' | 'hello' | 'none';

	interface Props {
		title: string;
		hint?: string;
		art?: Art;
		children?: Snippet;
	}

	let { title, hint, art = 'rest', children }: Props = $props();

	const POSE: Record<Exclude<Art, 'none'>, Pose> = {
		rest: 'asleep',
		search: 'searching',
		done: 'happy',
		lost: 'lost',
		failed: 'dizzy',
		hello: 'wave'
	};
</script>

<div class="empty">
	{#if art !== 'none'}
		<span class="pip"><Mascot pose={POSE[art]} size={128} /></span>
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
		padding: 44px 20px 72px;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--bg-elev) 60%, transparent);
	}

	.pip {
		display: block;
		filter: drop-shadow(0 12px 20px rgba(0, 0, 0, 0.28));
	}

	h3 {
		font-size: 1.15rem;
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
