<script lang="ts">
	import { titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import type { Kind, StudioDetail } from '#lib/types';

	let studio = $state<StudioDetail | null>(null);
	let loading = $state(true);
	let error = $state<Failure | null>(null);
	let only = $state<Kind | null>(null);
	let expanded = $state(false);

	const id = $derived(Number(page.params.id));

	$effect(() => {
		void load(id);
	});

	let token = 0;

	async function load(studioId: number) {
		const mine = ++token;
		loading = true;
		error = null;
		try {
			const res = await api.studio(studioId);
			if (mine !== token) return;
			studio = res;
		} catch (e) {
			if (mine !== token) return;
			error = failure(e, {
				title: "This studio isn't on TMDB",
				hint: 'The link may be wrong, or TMDB has removed it.'
			});
		} finally {
			if (mine === token) loading = false;
		}
	}

	const titles = $derived(uniqueBy(studio?.titles ?? [], titleKey));
	const films = $derived(titles.filter((t) => t.kind === 'movie').length);
	const shows = $derived(titles.filter((t) => t.kind === 'tv').length);
	const shown = $derived(titles.filter((t) => !only || t.kind === only));
	const logo = $derived(img(studio?.logo_path, 'w300'));
</script>

<svelte:head>
	<title>
		{studio?.name ? `${studio.name} · Frog` : error ? `${error.heading} · Frog` : 'Frog'}
	</title>
</svelte:head>

<div class="page">
	{#if loading && !studio}
		<Spinner label="Loading" />
	{:else if error}
		<Empty art={error.missing ? 'search' : 'rest'} title={error.title} hint={error.hint}>
			{#if !error.missing}
				<button class="btn primary" onclick={() => load(id)}>Try again</button>
			{/if}
			<a class="btn" class:primary={error.missing} href="/">Go home</a>
		</Empty>
	{:else if studio}
		<header class="head">
			<div class="mark" class:blank={!logo}>
				{#if logo}
					<img src={logo} alt="" />
				{:else}
					<span aria-hidden="true">{studio.name?.slice(0, 1) ?? '?'}</span>
				{/if}
			</div>
			<div class="info">
				<p class="eyebrow">Studio</p>
				<h1>{studio.name}</h1>
				{#if studio.headquarters || studio.country}
					<p class="muted">{studio.headquarters ?? studio.country}</p>
				{/if}
				{#if studio.description}
					<p class="bio" class:clamped={!expanded}>{studio.description}</p>
					<button class="btn ghost small" onclick={() => (expanded = !expanded)}>
						{expanded ? 'Show less' : 'Read more'}
					</button>
				{/if}
			</div>
		</header>

		{#if films && shows}
			<div class="chips" role="group" aria-label="Show">
				<button class="chip" class:on={!only} onclick={() => (only = null)}>All</button>
				<button class="chip" class:on={only === 'movie'} onclick={() => (only = 'movie')}>
					Films <span class="faint">{films}</span>
				</button>
				<button class="chip" class:on={only === 'tv'} onclick={() => (only = 'tv')}>
					Shows <span class="faint">{shows}</span>
				</button>
			</div>
		{/if}

		{#if !shown.length}
			<Empty art="search" title="Nothing listed yet" hint="TMDB has no titles for this studio." />
		{:else}
			<Grid>
				{#each shown as item (titleKey(item))}
					<Poster
						kindBadge={films > 0 && shows > 0}
						kind={item.kind}
						ref={item}
						name={item.name}
						posterPath={item.poster_path}
						fallbackPath={item.backdrop_path}
						year={item.year}
						status={item.in_library}
					/>
				{/each}
			</Grid>
		{/if}
	{/if}
</div>

<style>
	.head {
		display: flex;
		gap: 24px;
		align-items: center;
		margin-bottom: 32px;
	}

	/* Logos are drawn for white paper, often dark on transparent, so they sit
	   on a light tile in every theme. */
	.mark {
		flex: none;
		display: grid;
		place-items: center;
		width: 140px;
		height: 96px;
		padding: 14px;
		border-radius: var(--radius);
		background: #f6f4ec;
		border: 1px solid var(--line);
	}

	.mark img {
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
	}

	.mark.blank {
		background: var(--bg-elev-2);
		font-family: var(--font-display);
		font-size: 2.4rem;
		color: var(--text-dim);
	}

	.info {
		min-width: 0;
	}

	.info h1 {
		margin: 2px 0 4px;
	}

	.eyebrow {
		margin: 0;
		font-size: 0.72rem;
		font-weight: 650;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-faint);
	}

	.muted {
		margin: 0 0 8px;
	}

	.bio {
		max-width: 70ch;
		margin: 0 0 6px;
		color: var(--text-dim);
		line-height: 1.55;
	}

	.bio.clamped {
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.8rem;
	}

	.chips {
		display: flex;
		gap: 8px;
		margin-bottom: 20px;
	}

	@media (max-width: 720px) {
		.head {
			flex-direction: column;
			align-items: flex-start;
			gap: 16px;
		}

		.mark {
			width: 120px;
			height: 80px;
		}
	}
</style>
