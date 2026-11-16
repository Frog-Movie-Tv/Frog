<script lang="ts">
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import ProfileShell from '#lib/components/ProfileShell.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { prettyDate } from '#lib/format';
	import type { BadgeBoard, HeldBadge, LockedBadge } from '#lib/types';
	import { uniqueBy } from '#lib/unique';

	const username = $derived(page.params.username ?? '');
	let board = $state<BadgeBoard | null>(null);
	let failed = $state(false);
	let show = $state<'all' | 'earned' | 'locked'>('all');

	$effect(() => {
		const name = username;
		board = null;
		failed = false;
		void api
			.profileBadges(name)
			.then((b) => {
				if (name !== username) return;
				board = {
					...b,
					earned: uniqueBy(b.earned, (x) => x.id),
					locked: uniqueBy(b.locked, (x) => x.id)
				};
			})
			.catch(() => {
				if (name === username) failed = true;
			});
	});

	/** The day it was earned or given, as a sortable string. */
	const when = (b: HeldBadge) => b.earned_on ?? b.awarded_at.slice(0, 10);

	const earned = $derived(
		[...(board?.earned ?? [])].sort(
			(a, b) => when(b).localeCompare(when(a)) || b.awarded_at.localeCompare(a.awarded_at)
		)
	);

	const share = (b: LockedBadge) => (b.progress ? b.progress.have / b.progress.need : -1);
	/* The nearest first, so the next step of each ladder comes before the
	   ones above it; the server's order breaks ties. */
	const locked = $derived(
		(board?.locked ?? [])
			.map((b, i) => ({ b, i }))
			.sort((x, y) => share(y.b) - share(x.b) || x.i - y.i)
			.map(({ b }) => b)
	);

	const total = $derived(board?.total ?? 0);
	const percent = $derived(total ? Math.round((earned.length / total) * 100) : 0);
	const count = (n: number) => n.toLocaleString();

	function earnedText(b: HeldBadge) {
		if (!b.automatic) return `Given ${prettyDate(b.awarded_at)}`;
		const day = b.earned_on ? prettyDate(`${b.earned_on}T12:00:00`) : prettyDate(b.awarded_at);
		return `Earned ${day}`;
	}
</script>

<ProfileShell tab="badges">
	{#snippet children(profile)}
		{#if failed}
			<Empty art="rest" title="Badges could not be loaded" />
		{:else if board === null}
			<Spinner label="Loading badges" />
		{:else}
			<section class="summary card" aria-label="Badges earned">
				<p class="tally">
					<strong>{count(earned.length)}</strong>
					<span class="dim">of {count(total)} earned</span>
				</p>
				<div
					class="meter"
					role="progressbar"
					aria-label="Badges earned"
					aria-valuemin={0}
					aria-valuemax={total}
					aria-valuenow={earned.length}
				>
					<span style:width="{percent}%"></span>
				</div>
				<span class="percent faint">{percent}%</span>
			</section>

			<div class="segmented" role="group" aria-label="Show">
				<button class="chip" class:on={show === 'all'} onclick={() => (show = 'all')}>All</button>
				<button class="chip" class:on={show === 'earned'} onclick={() => (show = 'earned')}
					>Earned</button
				>
				<button class="chip" class:on={show === 'locked'} onclick={() => (show = 'locked')}
					>Still to earn</button
				>
			</div>

			{#if show !== 'locked'}
				<h2 class="heading">Earned <span class="faint">{count(earned.length)}</span></h2>
				{#if earned.length}
					<ul class="list">
						{#each earned as b, i (b.id)}
							<li class="row" style="--i: {Math.min(i, 20)}">
								<img class="art" src={b.small} alt="" width="64" height="64" loading="lazy" />
								<span class="body">
									<strong>{b.name}</strong>
									{#if b.description}<span class="about">{b.description}</span>{/if}
									{#if profile.me && !b.shown}
										<span class="faint note">Not on your profile</span>
									{/if}
								</span>
								<span class="when">
									<span>{earnedText(b)}</span>
									{#if b.earned_with}<span class="faint with" title={b.earned_with}
											>with {b.earned_with}</span
										>{/if}
								</span>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="faint none">
						{profile.me ? 'Your first one is not far off.' : 'None yet.'}
					</p>
				{/if}
			{/if}

			{#if show !== 'earned' && locked.length}
				<h2 class="heading">Still to earn <span class="faint">{count(locked.length)}</span></h2>
				<ul class="list">
					{#each locked as b, i (b.id)}
						<li class="row locked" style="--i: {Math.min(i, 20)}">
							<img class="art" src={b.small} alt="" width="64" height="64" loading="lazy" />
							<span class="body">
								<strong>{b.name}</strong>
								{#if b.description}<span class="about">{b.description}</span>{/if}
							</span>
							{#if b.progress}
								{@const p = b.progress}
								<span class="when progress">
									<span class="numbers">{count(p.have)} / {count(p.need)}</span>
									<span
										class="meter small"
										role="progressbar"
										aria-label="{b.name}: {count(p.have)} of {count(p.need)}"
										aria-valuemin={0}
										aria-valuemax={p.need}
										aria-valuenow={p.have}
									>
										<span style:width="{Math.min(100, (p.have / p.need) * 100)}%"></span>
									</span>
								</span>
							{/if}
						</li>
					{/each}
				</ul>
			{:else if show === 'locked'}
				<p class="faint none">Every badge earned.</p>
			{/if}
		{/if}
	{/snippet}
</ProfileShell>

<style>
	.summary {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
		padding: 16px 20px;
		margin-bottom: 16px;
	}

	.tally {
		margin: 0;
		display: flex;
		align-items: baseline;
		gap: 6px;
		white-space: nowrap;
	}

	.tally strong {
		font-family: var(--font-display);
		font-size: 1.6rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.dim {
		color: var(--text-dim);
	}

	.percent {
		font-variant-numeric: tabular-nums;
	}

	.meter {
		display: block;
		height: 8px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.meter > span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--accent);
	}

	.meter.small {
		height: 5px;
		width: 100%;
	}

	.segmented {
		margin-bottom: 8px;
	}

	.heading {
		margin: 22px 0 8px;
		font-size: 1rem;
		font-weight: 650;
	}

	.heading .faint {
		font-weight: 500;
		margin-left: 4px;
	}

	.none {
		margin: 4px 0 0;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 2px;
	}

	.row {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		animation: row-in 360ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--i) * 20ms);
	}

	@keyframes row-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}

	.art {
		width: 64px;
		height: 64px;
	}

	/* Still to earn: the picture in grey, so what is held stands out. */
	.locked .art {
		filter: grayscale(1);
		opacity: 0.45;
	}

	.locked strong {
		color: var(--text-dim);
	}

	.body {
		display: grid;
		gap: 2px;
		min-width: 0;
	}

	.about {
		color: var(--text-dim);
		font-size: 0.88rem;
	}

	.note {
		font-size: 0.78rem;
	}

	.when {
		display: grid;
		justify-items: end;
		gap: 2px;
		font-size: 0.82rem;
		color: var(--text-dim);
		text-align: right;
		min-width: 0;
		max-width: 24ch;
	}

	/* One line: a long title would otherwise stack the row up. */
	.with {
		max-width: 100%;
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.progress {
		width: 140px;
		gap: 6px;
	}

	.numbers {
		font-variant-numeric: tabular-nums;
	}

	/* A phone: the day or the meter goes under the words. */
	@media (max-width: 600px) {
		.summary {
			grid-template-columns: auto minmax(0, 1fr);
			padding: 14px 16px;
		}

		.summary .meter {
			grid-column: 1 / -1;
			grid-row: 2;
		}

		.percent {
			justify-self: end;
		}

		.row {
			grid-template-columns: 52px minmax(0, 1fr);
			gap: 4px 14px;
			padding: 10px;
		}

		.art {
			width: 52px;
			height: 52px;
			grid-row: span 2;
		}

		.when {
			justify-items: start;
			text-align: left;
			max-width: none;
		}

		.progress {
			width: 100%;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.row {
			animation: none;
		}
	}
</style>
