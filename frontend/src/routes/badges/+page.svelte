<script lang="ts">
	import { goto } from '$app/navigation';
	import { session } from '#lib/session.svelte';
	import Spinner from '#lib/components/Spinner.svelte';

	// Your badges live on your profile's Badges tab; this is the short way there.
	$effect(() => {
		const name = session.status?.username;
		if (name) void goto(`/people/${encodeURIComponent(name)}/badges`, { replaceState: true });
	});
</script>

<svelte:head><title>Badges · Frog</title></svelte:head>

<Spinner label="Opening your badges" />
