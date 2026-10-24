<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<div class="grid">{@render children()}</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--poster-min), 1fr));
		gap: calc(var(--gap) * 1.1) var(--gap);
	}
</style>
