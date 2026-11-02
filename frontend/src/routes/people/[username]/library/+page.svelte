<script lang="ts">
	import { page } from '$app/state';
	import LibraryBrowser from '#lib/components/LibraryBrowser.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';

	const username = $derived(page.params.username ?? '');
</script>

<ProfileShell tab="library">
	{#key username}
		<LibraryBrowser owner={username} embedded />
	{/key}
</ProfileShell>
