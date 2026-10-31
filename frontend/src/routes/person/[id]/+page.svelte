<script lang="ts">
	import { titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { prettyDate } from '#lib/format';
	import type { PersonDetail } from '#lib/types';

	let person = $state<PersonDetail | null>(null);
	let loading = $state(true);
	let error = $state<Failure | null>(null);
	let seenOnly = $state(false);
	let expanded = $state(false);

	const id = $derived(Number(page.params.id));

	$effect(() => {
		void load(id);
	});

	let token = 0;

	async function load(personId: number) {
		const mine = ++token;
		loading = true;
		error = null;
		try {
			const res = await api.person(personId);
			if (mine !== token) return;
			person = res;
		} catch (e) {
			if (mine !== token) return;
			error = failure(e, {
				title: "This person isn't on TMDB",
				hint: 'The link may be wrong, or TMDB has removed them.'
			});
		} finally {
			if (mine === token) loading = false;
		}
	}

	const shown = $derived(
		uniqueBy(person?.credits ?? [], (c) => titleKey(c.item)).filter(
			(c) => !seenOnly || c.item.in_library !== null
		)
	);
	const credits = $derived(shown.filter((c) => !c.appearance));
	const appearances = $derived(shown.filter((c) => c.appearance));
	const seenCount = $derived(
		(person?.credits ?? []).filter((c) => c.item.in_library !== null).length
	);
</script>

<svelte:head>
	<title>
		{person?.name ? `${person.name} · Frog` : error ? `${error.heading} · Frog` : 'Frog'}
	</title>
</svelte:head>

<div class="page">
	{#if loading && !person}
		<Spinner label="Loading" />
	{:else if error}
		<Empty art={error.missing ? 'search' : 'rest'} title={error.title} hint={error.hint}>
			{#if !error.missing}
				<button class="btn primary" onclick={() => load(id)}>Try again</button>
			{/if}
			<a class="btn" class:primary={error.missing} href="/">Go home</a>
		</Empty>
	{:else if person}
		<header class="head">
			<div class="photo">
				{#if person.profile_path}
					<img src={img(person.profile_path, 'w342')} alt="Portrait of {person.name}" />
				{:else}
					<span aria-hidden="true">{person.name?.slice(0, 1) ?? '?'}</span>
				{/if}
			</div>

			<div class="info">
				<h1>{person.name}</h1>
				<p class="muted sub">
					{#if person.known_for_department}<span>{person.known_for_department}</span>{/if}
					{#if person.birthday}
						<span class="dot" aria-hidden="true">•</span>
						<span
							>{prettyDate(person.birthday)}{person.deathday
								? ` – ${prettyDate(person.deathday)}`
								: ''}</span
						>
					{/if}
					{#if person.place_of_birth}
						<span class="dot" aria-hidden="true">•</span>
						<span>{person.place_of_birth}</span>
					{/if}
				</p>

				{#if person.biography}
					<p class="bio" class:clamped={!expanded}>{person.biography}</p>
					<button class="btn ghost small" onclick={() => (expanded = !expanded)}>
						{expanded ? 'Show less' : 'Read more'}
					</button>
				{/if}
			</div>
		</header>

		<div class="section-head">
			<h2>Filmography</h2>
			<button class="btn ghost small" class:on={seenOnly} onclick={() => (seenOnly = !seenOnly)}>
				{seenOnly ? 'Showing tracked' : `${seenCount} in your library`}
			</button>
		</div>

		{#if !shown.length}
			<Empty
				title={seenOnly ? 'Nothing of theirs is in your library yet' : 'No credits found'}
				hint={seenOnly ? 'Turn the filter off to see everything.' : undefined}
			/>
		{:else}
			<Grid>
				{#each credits as credit (titleKey(credit.item))}
					<div class="credit" class:seen={credit.item.in_library !== null}>
						<Poster
							kind={credit.item.kind}
							ref={credit.item}
							name={credit.item.name}
							posterPath={credit.item.poster_path}
							fallbackPath={credit.item.backdrop_path}
							year={credit.item.year}
							status={credit.item.in_library}
						/>
						{#if credit.role}
							<span class="role faint">{credit.role}</span>
						{/if}
					</div>
				{/each}
			</Grid>
		{/if}

		{#if appearances.length}
			<div class="appearances">
				<Rail title="Appearances">
					{#each appearances as credit (titleKey(credit.item))}
						<div class="credit" class:seen={credit.item.in_library !== null}>
							<Poster
								kind={credit.item.kind}
								ref={credit.item}
								name={credit.item.name}
								posterPath={credit.item.poster_path}
								fallbackPath={credit.item.backdrop_path}
								year={credit.item.year}
								status={credit.item.in_library}
							/>
						</div>
					{/each}
				</Rail>
			</div>
		{/if}
	{/if}
</div>

<style>
	.head {
		display: flex;
		gap: 24px;
		margin-bottom: 40px;
		align-items: flex-start;
	}

	.photo {
		flex: none;
		width: 150px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius);
		overflow: hidden;
		background: var(--bg-elev-2);
		display: grid;
		place-items: center;
		font-size: 2rem;
		color: var(--text-faint);
		box-shadow: var(--shadow-2);
	}

	.photo img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.info {
		display: flex;
		flex-direction: column;
		gap: 10px;
		align-items: flex-start;
		min-width: 0;
	}

	.sub {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		font-size: 0.86rem;
	}

	.dot {
		opacity: 0.5;
	}

	.bio {
		max-width: 76ch;
		font-size: 0.92rem;
		color: var(--text-dim);
		white-space: pre-line;
	}

	.bio.clamped {
		display: -webkit-box;
		-webkit-line-clamp: 4;
		line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.btn.small {
		padding: 6px 12px;
		font-size: 0.78rem;
	}

	.btn.on {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 50%, transparent);
	}

	.credit {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	/* Everything they made, with what I have seen pulled forward. */
	.credit:not(.seen) {
		opacity: 0.56;
		transition: opacity var(--fast) var(--ease);
	}

	.credit:not(.seen):hover {
		opacity: 1;
	}

	.role {
		font-size: 0.72rem;
		line-height: 1.3;
	}

	@media (max-width: 720px) {
		.head {
			flex-direction: column;
			gap: 16px;
		}

		.photo {
			width: 110px;
		}
	}

	.appearances {
		margin-top: 44px;
	}
</style>
