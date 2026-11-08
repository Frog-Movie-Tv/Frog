<script lang="ts">
	import { page } from '$app/state';
	import Score from '#lib/components/Score.svelte';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { titleHref } from '#lib/titles';
	import { statusLabel, type Kind, type RatedTitle } from '#lib/types';
	import { uniqueBy } from '#lib/unique';

	const username = $derived(page.params.username ?? '');
	let ratings = $state<RatedTitle[] | null>(null);
	let kind = $state<'all' | Kind>('all');

	$effect(() => {
		const name = username;
		ratings = null;
		void api
			.profileRatings(name)
			.then((r) => {
				if (name === username) ratings = uniqueBy(r.ratings, (t) => t.id);
			})
			.catch(() => (ratings = []));
	});

	const shown = $derived((ratings ?? []).filter((t) => kind === 'all' || t.kind === kind));
	const hasManga = $derived((ratings ?? []).some((t) => t.kind === 'manga'));
	const partName = (s: { season: number; first_episode: number }) =>
		(s.season === 0 ? 'Specials' : `S${s.season}`) +
		(s.first_episode > 1 ? ` from E${s.first_episode}` : '');
</script>

<ProfileShell tab="ratings">
	{#if ratings === null}
		<Spinner label="Loading ratings" />
	{:else if !ratings.length}
		<Empty art="rest" title="Nothing rated yet" />
	{:else}
		<div class="segmented" role="group" aria-label="Show">
			<button class="chip" class:on={kind === 'all'} onclick={() => (kind = 'all')}>All</button>
			<button class="chip" class:on={kind === 'movie'} onclick={() => (kind = 'movie')}
				>Films</button
			>
			<button class="chip" class:on={kind === 'tv'} onclick={() => (kind = 'tv')}>Shows</button>
			{#if hasManga}
				<button class="chip" class:on={kind === 'manga'} onclick={() => (kind = 'manga')}
					>Manga</button
				>
			{/if}
		</div>
		<ol class="rated">
			{#each shown as t, i (t.id)}
				{@const src = img(t.poster_path, 'w185')}
				<li style="--i: {Math.min(i, 20)}">
					<a href={titleHref(t) ?? `/t/${t.id}`} class="row">
						<span class="rank faint">{i + 1}</span>
						<span class="cover"
							>{#if src}<img {src} alt="" loading="lazy" />{/if}</span
						>
						<span class="body">
							<strong>{t.name}</strong>
							<span class="faint status">{statusLabel(t.status, t.kind)}</span>
							{#if t.seasons.length}
								<span class="seasons">
									{#each t.seasons as s (`${s.season}-${s.first_episode}`)}
										<span class="season">
											{partName(s)} <b><Score value={s.rating} empty="—" /></b>
										</span>
									{/each}
								</span>
							{/if}
						</span>
						<span class="score"><Score value={t.rating} empty="—" /></span>
					</a>
				</li>
			{/each}
		</ol>
	{/if}
</ProfileShell>

<style>
	.segmented {
		margin-bottom: 16px;
	}

	.rated {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 2px;
	}

	.rated li {
		animation: row-in 360ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--i) * 25ms);
	}

	@keyframes row-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}

	.row {
		display: grid;
		grid-template-columns: 2.4ch 46px minmax(0, 1fr) auto;
		align-items: center;
		gap: 14px;
		padding: 8px 10px;
		border-radius: var(--radius-sm);
		color: inherit;
		text-decoration: none;
		transition: background var(--fast) var(--ease);
	}

	.row:hover {
		background: var(--bg-elev);
	}

	.rank {
		text-align: right;
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}

	.cover {
		width: 46px;
		aspect-ratio: 2 / 3;
		border-radius: 6px;
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.body {
		display: grid;
		gap: 3px;
		min-width: 0;
	}

	.body strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.status {
		font-size: 0.78rem;
	}

	.seasons {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}

	.season {
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		font-size: 0.74rem;
		color: var(--text-dim);
	}

	.season b {
		color: var(--text);
		font-weight: 650;
	}

	.score {
		font-family: var(--font-display);
		font-size: 1.2rem;
		font-weight: 700;
		color: var(--accent);
		font-variant-numeric: tabular-nums;
	}

	@media (prefers-reduced-motion: reduce) {
		.rated li {
			animation: none;
		}
	}
</style>
