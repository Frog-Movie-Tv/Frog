<script lang="ts">
	import { img } from '#lib/api';
	import Score from '#lib/components/Score.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import { episodeCode, longRuntime, plural } from '#lib/format';
	import { titleHref } from '#lib/titles';
	import type { Profile, ProfileCard } from '#lib/types';

	function tiles(p: Profile) {
		return [
			{ label: 'Time watched', value: longRuntime(p.stats.minutes) },
			{ label: 'Films', value: String(p.stats.films) },
			{
				label: 'Episodes',
				value: String(p.stats.episodes),
				sub: plural(p.stats.shows, 'show')
			},
			...(p.stats.chapters
				? [{ label: 'Chapters', value: p.stats.chapters.toLocaleString() }]
				: []),
			{
				label: 'Library',
				value: String(p.stats.library),
				sub: `${p.stats.completed} finished`
			},
			{
				label: 'Average rating',
				value: '',
				rating: p.stats.mean_rating,
				sub: plural(p.stats.favourites, 'favourite')
			},
			{
				label: 'Streak',
				value: plural(p.stats.streak, 'day'),
				sub: `best ${plural(p.stats.longest_streak, 'day')}`
			}
		];
	}
</script>

{#snippet rail(title: string, cards: ProfileCard[])}
	{#if cards.length}
		<Rail {title}>
			{#each cards as card (card.id)}
				<Poster
					kind={card.kind}
					ref={card}
					name={card.name}
					posterPath={card.poster_path}
					rating={card.rating}
				/>
			{/each}
		</Rail>
	{/if}
{/snippet}

<ProfileShell tab="overview">
	{#snippet children(profile: Profile)}
		<div class="tiles">
			{#each tiles(profile) as t, i (t.label)}
				<div class="tile card" style="--i: {i}">
					<span class="eyebrow">{t.label}</span>
					<strong
						>{#if 'rating' in t}<Score value={t.rating} empty="—" />{:else}{t.value}{/if}</strong
					>
					{#if t.sub}<span class="faint">{t.sub}</span>{/if}
				</div>
			{/each}
		</div>

		{#if profile.genres.length}
			<ul class="genres" aria-label="Favourite genres">
				{#each profile.genres as g (g.name)}
					<li>{g.name} <span class="faint">{g.titles}</span></li>
				{/each}
			</ul>
		{/if}

		{#if profile.pinned.length}
			<section class="block">
				<h2>Pinned</h2>
				<div class="pins">
					{#each profile.pinned as card (card.id)}
						<Poster
							kind={card.kind}
							ref={card}
							name={card.name}
							posterPath={card.poster_path}
							rating={card.rating}
						/>
					{/each}
				</div>
			</section>
		{/if}

		{#if profile.watching.length}
			<!-- Where they are in each show, the way AniList lists what someone
			     is watching: the next episode and how far through. -->
			<section class="block">
				<h2>Watching now</h2>
				<ul class="watching">
					{#each profile.watching as card (card.id)}
						{@const src = img(card.poster_path, 'w185')}
						{@const done = card.progress?.total
							? Math.min(100, (card.progress.watched / card.progress.total) * 100)
							: 0}
						<li>
							<a href={titleHref(card) ?? `/t/${card.id}`} class="now card">
								<span class="cover"
									>{#if src}<img {src} alt="" loading="lazy" />{/if}</span
								>
								<span class="now-body">
									<strong>{card.name}</strong>
									{#if card.next}
										<span class="ep">
											On {episodeCode(card.next.season, card.next.episode)}
											{#if card.next.name}<span class="faint">· {card.next.name}</span>{/if}
										</span>
									{:else if card.kind === 'movie'}
										<span class="ep faint">Film</span>
									{:else if card.kind === 'manga' && card.progress}
										<span class="ep">Chapter {card.progress.watched + 1}</span>
									{/if}
									{#if card.progress && card.progress.total > 0}
										<span class="bar"><i style:width="{done}%"></i></span>
										<span class="faint count">
											{card.progress.watched} of {card.progress.total}
											{card.kind === 'manga' ? 'chapters' : 'episodes'}
										</span>
									{/if}
								</span>
							</a>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if !profile.watching.length && !profile.favourites.length && !profile.recent.length}
			<Empty art="rest" title="Nothing watched yet" />
		{/if}

		<div class="rails">
			{@render rail('Favourites', profile.favourites)}
			{@render rail('Recently watched', profile.recent)}
		</div>
	{/snippet}
</ProfileShell>

<style>
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 12px;
		margin-bottom: 18px;
	}

	.tile {
		padding: 16px 18px;
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
		animation: tile-in 420ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--i) * 45ms);
	}

	@keyframes tile-in {
		from {
			opacity: 0;
			transform: translateY(8px) scale(0.98);
		}
	}

	.tile strong {
		font-family: var(--font-display);
		font-size: 1.7rem;
		font-weight: 650;
		overflow-wrap: normal;
	}

	.tile .faint {
		font-size: 0.8rem;
	}

	.genres {
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0 0 28px;
		padding: 0;
	}

	.genres li {
		padding: 5px 12px;
		border-radius: 999px;
		background: var(--bg-elev);
		border: 1px solid var(--line);
		font-size: 0.82rem;
	}

	.block {
		margin-bottom: 32px;
	}

	h2 {
		font-size: 1.1rem;
		margin: 0 0 12px;
	}

	/* Fixed widths, as on every rail: one pin never stretches across. */
	.pins {
		display: grid;
		grid-template-columns: repeat(auto-fill, 150px);
		gap: 14px;
	}

	.watching {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 320px), 1fr));
		gap: 12px;
	}

	.now {
		display: flex;
		gap: 14px;
		padding: 10px;
		align-items: center;
		color: inherit;
		text-decoration: none;
		transition:
			transform var(--fast) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.now:hover {
		transform: translateY(-2px);
		border-color: var(--line-strong);
	}

	.cover {
		flex: none;
		width: 56px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.now-body {
		display: grid;
		gap: 4px;
		min-width: 0;
		flex: 1;
	}

	.now-body strong {
		font-size: 0.95rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ep {
		font-size: 0.84rem;
		font-weight: 600;
		color: var(--accent);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ep .faint {
		font-weight: 400;
	}

	.bar {
		display: block;
		height: 4px;
		border-radius: 999px;
		background: var(--line-strong);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width var(--slow) var(--ease);
	}

	.count {
		font-size: 0.76rem;
	}

	.rails {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	@media (max-width: 600px) {
		.tiles {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.tile strong {
			font-size: 1.4rem;
		}

		.pins {
			grid-template-columns: repeat(auto-fill, 112px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tile {
			animation: none;
		}
	}
</style>
