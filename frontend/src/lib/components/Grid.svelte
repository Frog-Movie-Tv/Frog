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

	/* Cards rise into place as a grid appears; the first rows a little
	   apart, the rest together, so a long list never waits on its tail. */
	.grid > :global(*) {
		animation: card-rise 240ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	.grid > :global(*:nth-child(n + 7)) {
		animation-delay: 40ms;
	}

	@keyframes card-rise {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}
</style>
