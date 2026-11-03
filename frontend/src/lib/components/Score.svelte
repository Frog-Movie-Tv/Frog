<script lang="ts">
	import { formatScore, toDisplay } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import Stars from './Stars.svelte';

	/*
	 * A rating, the way the chosen format shows it: drawn stars on five stars,
	 * words and numbers otherwise. Sized by the text around it.
	 */

	interface Props {
		/** 0..100, or null for none. */
		value: number | null | undefined;
		/** What none reads as. */
		empty?: string;
	}

	let { value, empty = '–' }: Props = $props();

	const format = $derived(settings.current.scoreFormat);
	const text = $derived(value == null ? empty : formatScore(value, format));
</script>

{#if format === 'star5' && value != null}
	<span class="score-stars"><Stars value={toDisplay(value, format) ?? 0} label={text} /></span>
{:else}{text}{/if}

<style>
	.score-stars {
		display: inline-flex;
		vertical-align: -0.12em;
	}

	.score-stars :global(svg) {
		width: 0.95em;
		height: 0.95em;
	}
</style>
