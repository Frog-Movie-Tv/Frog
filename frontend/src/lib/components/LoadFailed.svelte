<script lang="ts">
	import Empty from './Empty.svelte';
	import type { Failure } from '#lib/failure';

	interface Props {
		failed: Failure;
		onretry: () => void;
	}

	let { failed, onretry }: Props = $props();
</script>

<Empty art={failed.missing ? 'search' : 'rest'} title={failed.title} hint={failed.hint}>
	{#if !failed.missing}
		<button class="btn primary" onclick={onretry}>Try again</button>
	{/if}
</Empty>
