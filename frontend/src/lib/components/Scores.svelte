<script lang="ts">
	import type { TitleScore } from '#lib/types';

	interface Props {
		scores: TitleScore[];
	}

	let { scores }: Props = $props();

	const out =
		(over: number, digits = 1) =>
		(score: number) =>
			(score / over).toFixed(digits);
	const percent = (score: number) => `${Math.round(score)}%`;
	const plain = (score: number) => String(Math.round(score));

	/** Each site's name, and its score the way that site writes it. */
	const SITES: Record<string, { name: string; show: (score: number) => string }> = {
		imdb: { name: 'IMDb', show: out(10) },
		tomatoes: { name: 'Rotten Tomatoes', show: percent },
		popcorn: { name: 'RT Audience', show: percent },
		metacritic: { name: 'Metacritic', show: plain },
		metacriticuser: { name: 'Metacritic users', show: out(10) },
		letterboxd: { name: 'Letterboxd', show: out(20) },
		trakt: { name: 'Trakt', show: percent },
		tmdb: { name: 'TMDB', show: out(10) },
		anilist: { name: 'AniList', show: percent },
		myanimelist: { name: 'MyAnimeList', show: out(10) },
		rogerebert: { name: 'Roger Ebert', show: out(25) }
	};

	const shown = $derived(scores.filter((s) => SITES[s.source]));
</script>

{#if shown.length}
	<ul class="scores" aria-label="Scores on other sites">
		{#each shown as s (s.source)}
			<li>
				<strong>{SITES[s.source].show(s.score)}</strong>
				<span>{SITES[s.source].name}</span>
			</li>
		{/each}
	</ul>
{/if}

<style>
	.scores {
		display: flex;
		flex-wrap: wrap;
		gap: 6px 16px;
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: 0.84rem;
	}

	li {
		display: inline-flex;
		align-items: baseline;
		gap: 5px;
		color: var(--text-dim);
	}

	strong {
		color: var(--text);
		font-weight: 650;
	}
</style>
