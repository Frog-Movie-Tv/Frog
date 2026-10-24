<script lang="ts">
	import { settings } from '#lib/settings.svelte';

	interface Props {
		size?: number;
		/** Show the "Frog" wordmark beside the mascot. */
		word?: boolean;
	}

	let { size = 26, word = false }: Props = $props();

	const src = $derived(`/mascots/${settings.current.mascot}.svg`);
</script>

<span class="logo" style:--size="{size}px">
	<img {src} alt="" width={size} height={size} />
	{#if word}<span class="word">Frog</span>{/if}
</span>

<style>
	.logo {
		display: inline-flex;
		align-items: center;
		gap: 9px;
	}

	img {
		width: var(--size);
		height: var(--size);
		display: block;
		/* A soft pond glow, so the mark sits on dark and light alike. */
		filter: drop-shadow(0 2px 6px rgba(0, 0, 0, 0.35));
	}

	.word {
		font-family: var(--font-display);
		font-weight: 600;
		letter-spacing: -0.02em;
		font-size: calc(var(--size) * 0.76);
	}
</style>
