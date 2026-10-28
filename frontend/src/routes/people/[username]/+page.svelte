<script lang="ts">
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { formatScore, longRuntime, plural, prettyDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
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

	const banner = $derived(
		profile?.banner ?? (profile?.banner_backdrop ? img(profile.banner_backdrop, 'w1280') : null)
	);
	const shownName = $derived(profile?.display_name || profile?.username || '');
</script>

<svelte:head>
	<title>{profile ? `${shownName} · Frog` : 'Profile · Frog'}</title>
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
		<header class="hero card">
			<div class="banner" class:plain={!banner}>
				{#if banner}<img src={banner} alt="" />{/if}
			</div>
			<div class="identity">
				<span class="picture"
					><Avatar src={profile.avatar} name={profile.username} size={96} /></span
				>
				<div class="names">
					<h1>{shownName}</h1>
					<p class="faint">
						@{profile.username} · joined {prettyDate(profile.joined)}
					</p>
				</div>
				{#if profile.me}
					<a class="btn ghost edit" href="/settings#account">
						<Icon name="pencil" size={15} /> Edit profile
					</a>
				{/if}
			</div>
			{#if profile.bio}
				<p class="bio">{profile.bio}</p>
			{/if}
			{#if profile.me && !profile.public}
				<p class="private faint">
					Only you can see this. <a href="/settings#account">Show it to others</a>
				</p>
			{/if}
			{#if profile.genres.length}
				<ul class="genres" aria-label="Favourite genres">
					{#each profile.genres as g (g.name)}
						<li>{g.name}</li>
					{/each}
				</ul>
			{/if}
		</header>

		{#if profile.pinned.length}
			<section class="pinned">
				<h2>Pinned</h2>
				<div class="pins">
					{#each profile.pinned as card (card.kind + card.tmdb_id)}
						<Poster
							kind={card.kind}
							tmdbId={card.tmdb_id}
							name={card.name}
							posterPath={card.poster_path}
							rating={card.rating}
						/>
					{/each}
				</div>
			</section>
		{/if}

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
				<span class="faint">{plural(profile.stats.shows, 'show')}</span>
			</div>
			<div class="tile card">
				<span class="eyebrow">Library</span>
				<strong>{profile.stats.library}</strong>
				<span class="faint">{profile.stats.completed} finished</span>
			</div>
			<div class="tile card">
				<span class="eyebrow">Average rating</span>
				<strong
					>{profile.stats.mean_rating == null
						? '—'
						: formatScore(profile.stats.mean_rating, settings.current.scoreFormat)}</strong
				>
				<span class="faint">{plural(profile.stats.favourites, 'favourite')}</span>
			</div>
			<div class="tile card">
				<span class="eyebrow">Streak</span>
				<strong>{plural(profile.stats.streak, 'day')}</strong>
				<span class="faint">best {plural(profile.stats.longest_streak, 'day')}</span>
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

	.hero {
		margin: 12px 0 24px;
		overflow: hidden;
		padding-bottom: 18px;
	}

	.banner {
		height: clamp(140px, 24vw, 240px);
		background: var(--bg-elev-2);
	}

	.banner.plain {
		background: linear-gradient(135deg, var(--accent-soft), var(--bg-elev-2) 70%);
	}

	.banner img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	/* Pulled up over the banner, so it has to sit above it: the banner's
	   picture would otherwise paint over the profile picture. */
	.identity {
		position: relative;
		z-index: 1;
		display: flex;
		align-items: flex-end;
		gap: 16px;
		padding: 0 22px;
		margin-top: -44px;
		flex-wrap: wrap;
	}

	.picture :global(.avatar) {
		border: 4px solid var(--bg-elev);
	}

	.names {
		flex: 1;
		min-width: 180px;
	}

	.names h1 {
		margin: 0;
		font-size: clamp(1.5rem, 4vw, 2.1rem);
	}

	.names p {
		margin: 4px 0 2px;
		font-size: 0.88rem;
	}

	.edit {
		margin-bottom: 4px;
	}

	.bio {
		margin: 16px 22px 0;
		max-width: 68ch;
		white-space: pre-line;
		line-height: 1.55;
	}

	.private {
		margin: 12px 22px 0;
		font-size: 0.86rem;
	}

	.private a {
		color: var(--accent);
	}

	.genres {
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 14px 22px 0;
		padding: 0;
	}

	.genres li {
		padding: 4px 10px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		font-size: 0.8rem;
	}

	.pinned h2 {
		font-size: 1.05rem;
		margin: 0 0 10px;
	}

	.pinned {
		margin-bottom: 26px;
	}

	/* Fixed widths, as on every rail: one pin never stretches across. */
	.pins {
		display: grid;
		grid-template-columns: repeat(auto-fill, 150px);
		gap: 14px;
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

	.tile .faint {
		font-size: 0.8rem;
	}

	.rails {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	@media (max-width: 560px) {
		.identity {
			padding: 0 16px;
		}

		.bio,
		.private,
		.genres {
			margin-left: 16px;
			margin-right: 16px;
		}

		.pins {
			grid-template-columns: repeat(auto-fill, 120px);
		}
	}
</style>
