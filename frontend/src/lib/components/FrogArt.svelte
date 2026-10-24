<script lang="ts">
	import { settings } from '#lib/settings.svelte';

	interface Props {
		/** Hints which pose suits the moment. */
		mood?: 'search' | 'rest' | 'done';
		size?: number;
	}

	let { mood = 'rest', size = 96 }: Props = $props();

	// Reuse the chosen mascot where it fits the mood, so empty states look
	// like the rest of the app rather than stock clip art.
	const file = $derived(
		mood === 'rest' ? 'sleepy' : mood === 'done' ? 'popcorn' : settings.current.mascot
	);
</script>

<span class="art" style:--size="{size}px">
	<span class="pad" aria-hidden="true"></span>
	<img src="/mascots/{file}.svg" alt="" width={size} height={size} />
</span>

<style>
	.art {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: calc(var(--size) * 1.5);
		height: calc(var(--size) * 1.2);
	}

	/* A lily pad for the frog to sit on. */
	.pad {
		position: absolute;
		bottom: 2px;
		width: calc(var(--size) * 1.35);
		height: calc(var(--size) * 0.34);
		border-radius: 50%;
		background: radial-gradient(
			ellipse at center,
			color-mix(in srgb, var(--accent) 26%, transparent),
			transparent 70%
		);
	}

	img {
		position: relative;
		display: block;
		filter: drop-shadow(0 6px 14px rgba(0, 0, 0, 0.3));
	}
</style>
