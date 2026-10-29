<script lang="ts">
	import { page } from '$app/state';
	import Empty from '#lib/components/Empty.svelte';

	const missing = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{missing ? 'Not found' : 'Something went wrong'} · Frog</title>
</svelte:head>

<div class="page">
	{#if missing}
		<Empty art="search" title="Nothing here" hint="This page doesn't exist, or it has moved.">
			<a class="btn primary" href="/">Go home</a>
		</Empty>
	{:else}
		<Empty art="rest" title="Something went wrong" hint="Reload the page to try again.">
			<button class="btn primary" onclick={() => location.reload()}>Reload</button>
			<a class="btn" href="/">Go home</a>
		</Empty>
	{/if}
</div>
