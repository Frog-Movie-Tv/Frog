<script lang="ts">
	import { page } from '$app/state';
	import Score from '#lib/components/Score.svelte';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { titleHref } from '#lib/titles';
	import { type Comparison, type Profile, statusLabel } from '#lib/types';

	const username = $derived(page.params.username ?? '');
	let compared = $state<Comparison | null>(null);
	let failed = $state(false);

	$effect(() => {
		const name = username;
		compared = null;
		failed = false;
		void api
			.profileCompare(name)
			.then((c) => {
				if (name === username) compared = c;
			})
			.catch(() => (failed = true));
	});

	const firstName = (p: Profile) => p.display_name || p.username;
</script>

<ProfileShell tab="compare">
	{#snippet children(profile: Profile)}
		{#if failed}
			<Empty art="rest" title="Nothing to compare" hint="This is your own profile." />
		{:else if !compared}
			<Spinner label="Comparing" />
		{:else}
			<div class="summary">
				<div class="figure card">
					<span class="eyebrow">Taste match</span>
					<strong>{compared.taste ? `${compared.taste.percent}%` : '—'}</strong>
					<span class="faint">
						{compared.taste
							? `From ${compared.taste.shared} titles you both rated`
							: 'Rate five of the same titles to see one'}
					</span>
				</div>
				<div class="figure card">
					<span class="eyebrow">In common</span>
					<strong>{compared.both.length}</strong>
					<span class="faint">titles you both have</span>
				</div>
				<div class="figure card">
					<span class="eyebrow">Only {firstName(profile)}</span>
					<strong>{compared.only_theirs.length}</strong>
					<span class="faint">to look at</span>
				</div>
			</div>

			{#if compared.only_theirs.length}
				<section class="block">
					<Rail title="Only {firstName(profile)} has">
						{#each compared.only_theirs.slice(0, 60) as t (t.id)}
							<Poster
								kind={t.kind}
								ref={t}
								name={t.name}
								posterPath={t.poster_path}
								rating={t.rating}
							/>
						{/each}
					</Rail>
				</section>
			{/if}

			{#if compared.both.length}
				<section class="block">
					<h2>You both have</h2>
					<div class="head-row faint" aria-hidden="true">
						<span></span><span>You</span><span>{firstName(profile)}</span>
					</div>
					<ul class="both">
						{#each compared.both as b (b.title.id)}
							{@const src = img(b.title.poster_path, 'w185')}
							<li>
								<a href={titleHref(b.title) ?? `/t/${b.title.id}`} class="pair">
									<span class="title">
										<span class="cover"
											>{#if src}<img {src} alt="" loading="lazy" />{/if}</span
										>
										<strong>{b.title.name}</strong>
									</span>
									<span class="side">
										<b><Score value={b.mine.rating} empty="—" /></b>
										<span class="faint">{statusLabel(b.mine.status)}</span>
									</span>
									<span class="side">
										<b><Score value={b.theirs.rating} empty="—" /></b>
										<span class="faint">{statusLabel(b.theirs.status)}</span>
									</span>
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			{#if compared.only_mine.length}
				<section class="block">
					<Rail title="Only you have">
						{#each compared.only_mine.slice(0, 60) as t (t.id)}
							<Poster
								kind={t.kind}
								ref={t}
								name={t.name}
								posterPath={t.poster_path}
								rating={t.rating}
							/>
						{/each}
					</Rail>
				</section>
			{/if}
		{/if}
	{/snippet}
</ProfileShell>

<style>
	.summary {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
		gap: 12px;
		margin-bottom: 28px;
	}

	.figure {
		display: grid;
		gap: 4px;
		padding: 16px 18px;
	}

	.figure strong {
		font-family: var(--font-display);
		font-size: 2rem;
		font-weight: 700;
	}

	.figure:first-child strong {
		color: var(--accent);
	}

	.figure .faint {
		font-size: 0.8rem;
	}

	.block {
		margin-bottom: 30px;
	}

	h2 {
		font-size: 1.1rem;
		margin: 0 0 10px;
	}

	.head-row,
	.pair {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 96px 96px;
		gap: 12px;
		align-items: center;
	}

	.head-row {
		padding: 0 10px 6px;
		font-size: 0.78rem;
		font-weight: 650;
	}

	.both {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--line);
	}

	.pair {
		padding: 8px 10px;
		border-bottom: 1px solid var(--line);
		color: inherit;
		text-decoration: none;
		transition: background var(--fast) var(--ease);
	}

	.pair:hover {
		background: var(--bg-elev);
	}

	.title {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.title strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.92rem;
	}

	.cover {
		flex: none;
		width: 36px;
		aspect-ratio: 2 / 3;
		border-radius: 5px;
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.side {
		display: grid;
		font-size: 0.76rem;
	}

	.side b {
		font-size: 1rem;
		font-variant-numeric: tabular-nums;
	}

	/* Five stars fit their column on any screen, a little smaller than text. */
	.side b :global(.score-stars svg) {
		width: 14px;
		height: 14px;
	}

	@media (max-width: 600px) {
		.summary {
			grid-template-columns: repeat(3, minmax(0, 1fr));
			gap: 8px;
		}

		.figure {
			padding: 12px;
		}

		.figure strong {
			font-size: 1.5rem;
		}

		.figure .faint {
			display: none;
		}

		.head-row,
		.pair {
			grid-template-columns: minmax(0, 1fr) 72px 72px;
			gap: 8px;
		}

		.side b :global(.score-stars svg) {
			width: 12px;
			height: 12px;
		}
	}
</style>
