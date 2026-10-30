<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';

	/*
	 * A title as a provider's address names it: /title/movie/550,
	 * /title/tv/1399, /title/anilist/21. Search results, credits and links
	 * made before titles had addresses of their own arrive here; the title
	 * is opened (and added to Frog if it is new) and the page moves to its
	 * own address, /t/{id}, without leaving this one in the history.
	 */
	const kind = $derived(page.params.kind ?? '');
	const raw = $derived(page.params.id ?? '');

	let error = $state<Failure | null>(null);
	let token = 0;

	const NOT_ON_TMDB = {
		title: "This title isn't on TMDB",
		hint: 'The link may be wrong, or TMDB has removed it.'
	};

	$effect(() => {
		void open(kind, raw);
	});

	async function open(k: string, idText: string) {
		const mine = ++token;
		error = null;
		// AniList-only titles once had negative ids here; they still open.
		const id = /^-?\d{1,12}$/.test(idText) ? Number(idText) : 0;
		const known = k === 'movie' || k === 'tv' || k === 'anilist';
		if (!known || id === 0 || (k === 'anilist' && id < 0)) {
			error = {
				missing: true,
				heading: 'Not found',
				title: known ? NOT_ON_TMDB.title : "That isn't a film or a show",
				hint: known ? NOT_ON_TMDB.hint : 'Check the address, or search for it instead.'
			};
			return;
		}
		try {
			const detail = await api.titleAt(k as 'movie' | 'tv' | 'anilist', id);
			if (mine !== token) return;
			await goto(`/t/${detail.item.id}`, { replaceState: true });
		} catch (e) {
			if (mine !== token) return;
			error = failure(e, NOT_ON_TMDB);
		}
	}
</script>

<svelte:head>
	<title>{error ? `${error.heading} · Frog` : 'Frog'}</title>
</svelte:head>

<div class="page">
	{#if error}
		<Empty art={error.missing ? 'search' : 'rest'} title={error.title} hint={error.hint}>
			{#if !error.missing}
				<button class="btn primary" onclick={() => open(kind, raw)}>Try again</button>
			{/if}
			<a class="btn" class:primary={error.missing} href="/">Go home</a>
		</Empty>
	{:else}
		<Spinner label="Loading" />
	{/if}
</div>
