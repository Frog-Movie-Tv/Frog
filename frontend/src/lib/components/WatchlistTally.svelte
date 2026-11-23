<script lang="ts">
	import { doneBy } from '#lib/books';
	import { plural, runtime } from '#lib/format';
	import type { Length, LibraryItem } from '#lib/types';

	interface Props {
		items: LibraryItem[];
		lengths: Record<string, Length>;
	}

	let { items, lengths }: Props = $props();

	/** Hours a day, for when it is done. */
	let pace = $state(2);
	const PACES = [1, 2, 3];

	const sum = $derived.by(() => {
		let total = 0,
			films = 0,
			filmsLeft = 0,
			shows = 0,
			showsLeft = 0,
			episodes = 0;
		for (const item of items) {
			const l = lengths[String(item.id)];
			if (!l) continue;
			total += l.total;
			if (!l.left) continue;
			if (item.kind === 'tv') {
				shows++;
				showsLeft += l.left;
				episodes += l.episodes_left;
			} else {
				films++;
				filmsLeft += l.left;
			}
		}
		const left = filmsLeft + showsLeft;
		return { total, left, watched: total - left, films, filmsLeft, shows, showsLeft, episodes };
	});

	const share = (minutes: number) => (sum.total ? `${(minutes / sum.total) * 100}%` : '0%');
	const day = $derived(
		doneBy(sum.left, pace, new Date()).toLocaleDateString(undefined, {
			weekday: 'long',
			day: 'numeric',
			month: 'long'
		})
	);
</script>

{#if sum.total > 0}
	<section class="tally" aria-label="Time left">
		<div>
			{#if sum.left > 0}
				<p class="big">
					{Math.floor(sum.left / 60)}<small>h</small>
					{String(sum.left % 60).padStart(2, '0')}<small>m</small>
				</p>
				<p class="cap">left to watch, {plural(sum.films + sum.shows, 'title')}</p>
			{:else}
				<p class="big">All watched</p>
				<p class="cap">{runtime(sum.total)} in all</p>
			{/if}
		</div>
		<div class="side">
			<div class="meter" aria-hidden="true">
				<i class="w" style:width={share(sum.watched)}></i>
				<i class="f" style:width={share(sum.filmsLeft)}></i>
				<i class="s" style:width={share(sum.showsLeft)}></i>
			</div>
			<p class="legend">
				{#if sum.watched}<span><i class="dot w"></i>Watched <b>{runtime(sum.watched)}</b></span
					>{/if}
				{#if sum.films}<span
						><i class="dot f"></i>{plural(sum.films, 'film')} <b>{runtime(sum.filmsLeft)}</b></span
					>{/if}
				{#if sum.shows}<span
						><i class="dot s"></i>{plural(sum.shows, 'show')}, {plural(sum.episodes, 'episode')}
						<b>{runtime(sum.showsLeft)}</b></span
					>{/if}
			</p>
			{#if sum.left > 0}
				<p class="pace">
					<span>At</span>
					<span class="segmented" role="group" aria-label="Hours a day">
						{#each PACES as hours (hours)}
							<button class:on={pace === hours} onclick={() => (pace = hours)}>{hours}h</button>
						{/each}
					</span>
					<span>a day, it’s done by <b>{day}</b>.</span>
				</p>
			{/if}
		</div>
	</section>
{/if}

<style>
	.tally {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: center;
		gap: 10px 34px;
		margin: 0 0 26px;
		padding: 22px 26px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background:
			linear-gradient(
				120deg,
				color-mix(in srgb, var(--glow, var(--accent)) 16%, transparent),
				transparent 55%
			),
			var(--bg-elev);
		backdrop-filter: var(--frost);
		-webkit-backdrop-filter: var(--frost);
		animation: settle 460ms var(--spring) 140ms backwards;
	}

	@keyframes settle {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}

	p {
		margin: 0;
	}

	.big {
		font-size: 2.8rem;
		font-weight: 800;
		letter-spacing: -0.03em;
		line-height: 1;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.big small {
		margin-left: 2px;
		color: var(--text-dim);
		font-size: 0.5em;
		font-weight: 700;
		letter-spacing: 0;
	}

	.cap {
		margin-top: 6px;
		color: var(--text-dim);
		font-size: 0.86rem;
	}

	.side {
		display: grid;
		gap: 14px;
		min-width: 0;
	}

	.meter {
		display: flex;
		height: 10px;
		overflow: hidden;
		border-radius: var(--radius-pill);
		background: var(--line);
	}

	.meter i {
		height: 100%;
		transition: width 900ms var(--ease);
	}

	.w {
		background: var(--good);
	}

	.f {
		background: var(--glow, var(--accent));
	}

	.s {
		background: color-mix(in srgb, var(--glow, var(--accent)) 55%, transparent);
	}

	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 6px 18px;
		color: var(--text-dim);
		font-size: 0.84rem;
	}

	.legend span {
		display: inline-flex;
		align-items: center;
		gap: 7px;
	}

	.legend b,
	.pace b {
		color: var(--text);
		font-variant-numeric: tabular-nums;
	}

	.dot {
		display: inline-block;
		width: 9px;
		height: 9px;
		border-radius: 50%;
	}

	.pace {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		color: var(--text-dim);
		font-size: 0.86rem;
	}

	.pace .segmented > button {
		min-height: 28px;
		padding: 4px 12px;
	}

	@media (max-width: 720px) {
		.tally {
			grid-template-columns: minmax(0, 1fr);
			padding: 18px;
		}

		.big {
			font-size: 2.3rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tally {
			animation: none;
		}
	}
</style>
