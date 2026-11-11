<script lang="ts">
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import LoadMore from '#lib/components/LoadMore.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { titleMenu } from '#lib/menus';
	import { titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import type { SearchItem } from '#lib/types';

	const CANNOT_LOAD = { title: 'Could not find more like this', hint: 'Try again in a minute.' };

	const id = $derived(Number(page.params.id));

	let name = $state('');
	let items = $state<SearchItem[]>([]);
	let next = $state<number | null>(null);
	let loading = $state(true);
	let growing = $state(false);
	let failed = $state<Failure | null>(null);
	let token = 0;

	$effect(() => {
		void load(id);
	});

	async function load(titleId: number) {
		const mine = ++token;
		loading = true;
		failed = null;
		try {
			const answer = await api.moreLike(titleId, 1);
			if (mine !== token) return;
			name = answer.seed.name;
			items = uniqueBy(answer.items, titleKey);
			next = answer.next_page;
		} catch (e) {
			if (mine === token) failed = failure(e, CANNOT_LOAD);
		} finally {
			if (mine === token) loading = false;
		}
	}

	async function more() {
		if (next === null || growing) return;
		growing = true;
		const mine = token;
		try {
			const answer = await api.moreLike(id, next);
			if (mine !== token) return;
			items = uniqueBy([...items, ...answer.items], titleKey);
			next = answer.next_page;
		} catch {
			next = null;
		} finally {
			growing = false;
		}
	}

	/** A right-click's offer on a title; the list reads itself again after. */
	const menuOf = (item: SearchItem) =>
		titleMenu({ ...item, status: item.in_library }, { onchanged: () => load(id) });
</script>

<svelte:head>
	<title>{name ? `More like ${name}` : 'More like this'} · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<div>
			<h1>More like this</h1>
			{#if name}<p class="sub">Because you watched <a href="/t/{id}">{name}</a></p>{/if}
		</div>
	</div>

	{#if loading}
		<Spinner label="Finding more like this" />
	{:else if failed}
		<LoadFailed {failed} onretry={() => load(id)} />
	{:else if !items.length}
		<Empty
			art="search"
			title="Nothing more to show"
			hint="TMDB has no recommendations for this one."
		>
			<a class="btn" href="/t/{id}">Back to the title</a>
		</Empty>
	{:else}
		<Grid>
			{#each items as item (titleKey(item))}
				<Poster
					menu={menuOf(item)}
					kind={item.kind}
					ref={item}
					name={item.name}
					posterPath={item.poster_path}
					year={item.year}
					status={item.in_library}
				/>
			{/each}
		</Grid>
		<LoadMore more={next !== null} loading={growing} onmore={more} />
	{/if}
</div>

<style>
	.sub a {
		color: var(--text);
		text-decoration: underline;
		text-decoration-color: var(--line-strong);
	}
</style>
