<script lang="ts">
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { longRuntime, relativeDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import type { Profile, ProfileCard } from '#lib/types';

	const username = $derived(page.params.username ?? '');

	let profile = $state<Profile | null>(null);
	let loading = $state(true);
	let missing = $state(false);

	$effect(() => {
		if (!session.authenticated || !username) return;
		loading = true;
		missing = false;
		void api
			.profile(username)
			.then((p) => (profile = p))
			.catch(() => {
				profile = null;
				missing = true;
			})
			.finally(() => (loading = false));
	});
</script>

<svelte:head>
	<title>{profile ? `${profile.username} · Frog` : 'Profile · Frog'}</title>
</svelte:head>

{#snippet rail(title: string, cards: ProfileCard[])}
	{#if cards.length}
		<Rail {title}>
			{#each cards as card (card.kind + card.tmdb_id)}
				<Poster
					kind={card.kind}
					tmdbId={card.tmdb_id}
					name={card.name}
					posterPath={card.poster_path}
					rating={card.rating}
				/>
			{/each}
		</Rail>
	{/if}
{/snippet}

<div class="page">
	<a class="back faint" href="/people">People</a>

	{#if loading}
		<Spinner label="Loading profile" />
	{:else if missing || !profile}
		<Empty
			art="search"
			title="No profile here"
			hint="It may be private, or there is no such account on this Frog."
		>
			<a class="btn" href="/people">Everyone sharing</a>
		</Empty>
	{:else}
		<header class="head">
			<span class="avatar" aria-hidden="true">{profile.username.slice(0, 1).toUpperCase()}</span>
			<div>
				<h1>{profile.username}</h1>
				<p class="faint">
					Joined {relativeDate(profile.joined)}
					{#if profile.me && !profile.public}
						· Only you can see this. <a href="/settings">Show it to others</a>
					{/if}
				</p>
			</div>
		</header>

		<div class="tiles">
			<div class="tile card">
				<span class="eyebrow">Time watched</span>
				<strong>{longRuntime(profile.stats.minutes)}</strong>
			</div>
			<div class="tile card">
				<span class="eyebrow">Films</span>
				<strong>{profile.stats.films}</strong>
			</div>
			<div class="tile card">
				<span class="eyebrow">Episodes</span>
				<strong>{profile.stats.episodes}</strong>
			</div>
		</div>

		{#if !profile.watching.length && !profile.favourites.length && !profile.recent.length}
			<Empty art="rest" title="Nothing watched yet" />
		{/if}

		<div class="rails">
			{@render rail('Watching', profile.watching)}
			{@render rail('Favourites', profile.favourites)}
			{@render rail('Recently watched', profile.recent)}
		</div>
	{/if}
</div>

<style>
	.back {
		font-size: 0.86rem;
		text-decoration: none;
	}

	.head {
		display: flex;
		align-items: center;
		gap: 16px;
		margin: 12px 0 22px;
	}

	.head p {
		margin: 4px 0 0;
		font-size: 0.88rem;
	}

	.head a {
		color: var(--accent);
	}

	.avatar {
		width: 64px;
		height: 64px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--accent-soft);
		color: var(--accent);
		font-size: 1.6rem;
		font-weight: 700;
		flex: none;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 12px;
		margin-bottom: 26px;
	}

	.tile {
		padding: 16px 18px;
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.tile strong {
		font-family: var(--font-display);
		font-size: 1.7rem;
		font-weight: 650;
	}

	.rails {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
</style>
