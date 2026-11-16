<script lang="ts">
	import { goto } from '$app/navigation';
	import Spinner from '#lib/components/Spinner.svelte';

	// The diary is the Calendar's Diary view now; old links and bookmarks land there.
	$effect(() => {
		void goto('/calendar?view=diary', { replace: true });
	});
</script>

<svelte:head><title>Diary · Frog</title></svelte:head>

<Spinner label="Opening your diary" />
