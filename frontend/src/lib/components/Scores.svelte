<script lang="ts">
	import type { TitleScore } from '#lib/types';

	interface Props {
		scores: TitleScore[];
	}

	let { scores }: Props = $props();

	/** The sites a title page shows, in this order. Scores arrive 0–100 and
	 *  are shown that way, as a percentage, whatever scale the site uses. */
	const MAIN = ['anilist', 'imdb', 'tomatoes', 'metacritic'] as const;
	/** Read through MDBList, which has a daily limit. */
	const LIMITED = ['imdb', 'tomatoes', 'metacritic'];

	const NAMES: Record<string, string> = {
		anilist: 'AniList',
		imdb: 'IMDb',
		tomatoes: 'Rotten Tomatoes',
		metacritic: 'Metacritic',
		tmdb: 'TMDB'
	};

	/**
	 * AniList, IMDb, Rotten Tomatoes and Metacritic. When none of the three
	 * MDBList carries is in (its daily limit, or nothing found yet), TMDB's own
	 * score stands in beside AniList's, so a page is never bare.
	 */
	const shown = $derived.by(() => {
		const by = (source: string) => scores.find((s) => s.source === source && s.score > 0);
		const list = MAIN.map(by).filter((s): s is TitleScore => !!s);
		if (!LIMITED.some((source) => by(source))) {
			const tmdb = by('tmdb');
			if (tmdb) list.push(tmdb);
		}
		return list;
	});

	const percent = (s: TitleScore) => Math.round(Math.max(0, Math.min(100, s.score)));

	function votes(s: TitleScore): string {
		if (!s.votes) return '';
		return ` from ${s.votes.toLocaleString()} ${s.votes === 1 ? 'rating' : 'ratings'}`;
	}

	/** Metacritic colours its box by how good the score is. */
	const metaShade = (n: number) => (n >= 61 ? '#00ce7a' : n >= 40 ? '#ffbd3f' : '#ff6874');
</script>

{#if shown.length}
	<ul class="scores" aria-label="Scores on other sites">
		{#each shown as s (s.source)}
			{@const n = percent(s)}
			<li title="{NAMES[s.source]}: {n}%{votes(s)}">
				<span class="mark" aria-hidden="true">
					{#if s.source === 'imdb'}
						<svg viewBox="0 0 48 24" width="40" height="20">
							<rect width="48" height="24" rx="4" fill="#f5c518" />
							<text
								x="24"
								y="17"
								text-anchor="middle"
								font-family="Arial Black, Arial, sans-serif"
								font-weight="900"
								font-size="13"
								fill="#000">IMDb</text
							>
						</svg>
					{:else if s.source === 'tomatoes'}
						{#if n >= 60}
							<!-- Fresh: a tomato. -->
							<svg viewBox="0 0 24 24" width="20" height="20">
								<circle cx="12" cy="13.5" r="9" fill="#fa320a" />
								<path
									d="M12 5.2c-1.2-1.8-3.4-2.4-5-1.6 1.5.3 2.6 1.1 3.1 2.2-1.6-.4-3.4.1-4.4 1.3 1.9-.1 3.5.3 4.6 1.1 0 0 1.2-1.4 1.7-3z"
									fill="#00912d"
								/>
								<path
									d="M12 5.2c1.2-1.8 3.4-2.4 5-1.6-1.5.3-2.6 1.1-3.1 2.2 1.6-.4 3.4.1 4.4 1.3-1.9-.1-3.5.3-4.6 1.1 0 0-1.2-1.4-1.7-3z"
									fill="#00912d"
								/>
							</svg>
						{:else}
							<!-- Rotten: a splat. -->
							<svg viewBox="0 0 24 24" width="20" height="20">
								<path
									d="M12 2.5l1.6 4.2 3.9-2.6-1 4.5 4.6-.2-3.4 3.1 3.8 2.6-4.6.8 1.7 4.3-4.1-2.3-1 4.6-1.9-4.2-3.4 3.2.4-4.6-4.5.7 3.1-3.4-4-2.3 4.6-.9-1.6-4.3 4.1 2.2z"
									fill="#0ac855"
								/>
							</svg>
						{/if}
					{:else if s.source === 'metacritic'}
						<svg viewBox="0 0 24 24" width="20" height="20">
							<rect width="24" height="24" rx="5" fill={metaShade(n)} />
							<text
								x="12"
								y="17"
								text-anchor="middle"
								font-family="Arial Black, Arial, sans-serif"
								font-weight="900"
								font-size="14"
								fill="#000">M</text
							>
						</svg>
					{:else if s.source === 'anilist'}
						<svg viewBox="0 0 24 24" width="20" height="20">
							<rect width="24" height="24" rx="5" fill="#1e2630" />
							<path d="M6.2 18.5 10.4 5.5h3.2l4.2 13h-3l-.9-2.9h-4.1l-.9 2.9z" fill="#fff" />
							<path d="M10.6 13.1h2.8L12 8.6z" fill="#1e2630" />
							<path d="M15.6 5.5h2.6v10.3h-2.6z" fill="#02a9ff" />
						</svg>
					{:else if s.source === 'tmdb'}
						<svg viewBox="0 0 48 24" width="40" height="20">
							<defs>
								<linearGradient id="tmdb-mark" x1="0" x2="1">
									<stop offset="0" stop-color="#90cea1" />
									<stop offset="1" stop-color="#01b4e4" />
								</linearGradient>
							</defs>
							<rect width="48" height="24" rx="6" fill="#0d253f" />
							<text
								x="24"
								y="16.5"
								text-anchor="middle"
								font-family="Arial Black, Arial, sans-serif"
								font-weight="900"
								font-size="12"
								fill="url(#tmdb-mark)">TMDB</text
							>
						</svg>
					{/if}
				</span>
				<strong>{n}<small>%</small></strong>
				<span class="site">{NAMES[s.source]}</span>
			</li>
		{/each}
	</ul>
{/if}

<style>
	/* Plain marks and numbers, set like the line of facts above them. */
	.scores {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 20px;
		margin: 2px 0 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: inline-flex;
		align-items: center;
		gap: 7px;
	}

	.mark {
		display: flex;
	}

	.mark svg {
		display: block;
	}

	strong {
		font-family: var(--font-display);
		font-size: 0.98rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
		color: var(--text);
		line-height: 1;
	}

	small {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--text-dim);
		margin-left: 1px;
	}

	/* The mark says which site; the name is there for anyone who does not
	   know the mark, and for screen readers. */
	.site {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
</style>
