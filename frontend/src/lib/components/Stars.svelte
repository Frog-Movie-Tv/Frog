<script lang="ts">
	import { ICONS } from './Icon.svelte';

	interface Props {
		/** Stars out of five, in halves. */
		value: number;
		size?: number;
		/** Read out instead of the stars, when they are all a control says. */
		label?: string;
		/** How many in all, the rest as outlines: a meter, not a count. */
		outOf?: number;
	}

	// Always the whole row: three stars read as three of five, not as three.
	let { value, size = 16, label, outOf = 5 }: Props = $props();

	// The clip that cuts the last star in half is this instance's own.
	const uid = $props.id();
	const full = $derived(Math.floor(value + 1e-9));
	const half = $derived(value - full >= 0.5 - 1e-9);
	const empty = $derived(outOf ? Math.max(0, outOf - full - (half ? 1 : 0)) : 0);
</script>

<!--
	Stars drawn rather than typed: a whole star for each point, a half star
	that is the same star filled to its middle, and the rest of the five as
	outlines, so every rating is drawn on the same row.
-->
<span
	class="stars"
	role={label ? 'img' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : 'true'}
>
	{#each { length: full }, i (i)}
		<svg viewBox="0 0 24 24" width={size} height={size}>
			<path d={ICONS.star} fill="currentColor" />
		</svg>
	{/each}
	{#if half}
		<svg viewBox="0 0 24 24" width={size} height={size}>
			<defs>
				<clipPath id="{uid}-half"><rect x="0" y="0" width="12" height="24" /></clipPath>
			</defs>
			<path
				d={ICONS.star}
				fill="none"
				stroke="currentColor"
				stroke-width="1.6"
				stroke-linejoin="round"
				opacity="0.5"
			/>
			<path d={ICONS.star} fill="currentColor" clip-path="url(#{uid}-half)" />
		</svg>
	{/if}
	{#each { length: empty }, i (i)}
		<svg class="empty" viewBox="0 0 24 24" width={size} height={size}>
			<path
				d={ICONS.star}
				fill="none"
				stroke="currentColor"
				stroke-width="1.6"
				stroke-linejoin="round"
			/>
		</svg>
	{/each}
</span>

<style>
	.stars {
		display: inline-flex;
		align-items: center;
		gap: 1px;
		line-height: 0;
	}

	svg {
		display: block;
		flex: none;
	}

	.empty {
		opacity: 0.4;
	}
</style>
