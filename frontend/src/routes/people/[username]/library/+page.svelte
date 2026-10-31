<script lang="ts">
	import { page } from '$app/state';
	import LibraryBrowser from '#lib/components/LibraryBrowser.svelte';

	const username = $derived(page.params.username ?? '');
</script>

<svelte:head>
	<title>{username} · Library · Frog</title>
</svelte:head>

{#key username}
	<LibraryBrowser owner={username} />
{/key}
