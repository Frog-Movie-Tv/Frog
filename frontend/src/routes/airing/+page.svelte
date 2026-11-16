<script lang="ts">
	import { goto } from '$app/navigation';
	import Spinner from '#lib/components/Spinner.svelte';

	// Airing is the Calendar's Airing view now; old links and bookmarks land there.
	$effect(() => {
		void goto('/calendar?view=airing', { replace: true });
	});
</script>

<svelte:head>
	<title>Airing · Frog</title>
</svelte:head>

<Spinner label="Opening Airing" />
