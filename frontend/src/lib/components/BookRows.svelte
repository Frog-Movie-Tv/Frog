<script module lang="ts">
	/** A little past, then back: a poster lands in its new place. */
	function settle(t: number) {
		const s = 1.15;
		return 1 + (s + 1) * (t - 1) ** 3 + s * (t - 1) ** 2;
	}
</script>

<script lang="ts">
	import { flip } from 'svelte/animate';
	import { plural, runtime, year } from '#lib/format';
	import type { MenuSource } from '#lib/menu.svelte';
	import type { Length, LibraryItem } from '#lib/types';
	import { statusLabel } from '#lib/types';
	import Icon from './Icon.svelte';
	import Poster from './Poster.svelte';

	interface Props {
		items: LibraryItem[];
		lengths: Record<string, Length>;
		/** A watchlist says what is left of each title. */
		watchlist?: boolean;
		/** A smart shelf's edge has a light running along it. */
		sheen?: boolean;
		/** Poster width in pixels. */
		size: number;
		/** Opened from the bookcase: the first titles carry its books' names. */
		named?: boolean;
		menuFor: (item: LibraryItem) => MenuSource;
	}

	let {
		items,
		lengths,
		watchlist = false,
		sheen = false,
		size,
		named = false,
		menuFor
	}: Props = $props();

	/** Between posters, half on each side of a cell, so the shelf runs on under them. */
	const GAP = 22;
	let width = $state(0);
	const per = $derived(Math.max(2, Math.floor(width / (size + GAP))));
	/** With three or fewer to a row the space left over is most of a poster,
	 *  so a narrow shelf (a phone's) widens them a little to fill it. */
	const wide = $derived(per <= 3 ? Math.max(size, Math.min(size * 1.25, width / per - GAP)) : size);
	/** A bookend closes the last shelf when there is room for one. */
	const bookend = $derived(items.length % per !== 0);

	const length = (item: LibraryItem) => lengths[String(item.id)];
	/** Today as TMDB writes a day, to tell what is not out yet. */
	const today = new Date().toISOString().slice(0, 10);
</script>

<div class="rows" bind:clientWidth={width} style:--size="{Math.floor(wide)}px" style:--per={per}>
	{#if width}
		{#each items as item, i (item.id)}
			{@const l = length(item)}
			{@const done = watchlist && l?.watched}
			<div
				class="cell"
				class:first={i % per === 0}
				class:last={i % per === per - 1 || (i === items.length - 1 && !bookend)}
				animate:flip={{ duration: 560, easing: settle }}
				style:--k={i}
			>
				<div
					class="book"
					class:done
					style:view-transition-name={named && i < 48 ? `book-${item.id}` : undefined}
				>
					<Poster
						bare
						morph
						menu={menuFor(item)}
						kind={item.kind}
						ref={item}
						name={item.name}
						posterPath={item.poster_path}
						rating={item.entry?.rating ?? null}
						watched={item.watched_episodes}
						total={item.total_episodes}
					/>
					{#if done}
						<span class="seen" title="Watched"><Icon name="check" size={15} /></span>
					{:else if watchlist && l?.left}
						<span class="chip">{runtime(l.left)}</span>
					{/if}
				</div>
				<div class="ledge" class:sheen>
					<div class="top"></div>
					<div class="front">
						<div class="card">
							<strong title={item.name}>{item.name}</strong>
							<span>
								{year(item.release_date)}{#if watchlist}{#if done}
										<em class="ok">{year(item.release_date) ? ' · ' : ''}Watched</em>
									{:else if item.kind === 'tv' && l?.episodes_left}
										<em class="lft"
											>{year(item.release_date) ? ' · ' : ''}{plural(l.episodes_left, 'episode')} left</em
										>
									{:else if l?.left}
										<em class="lft">{year(item.release_date) ? ' · ' : ''}{runtime(l.left)}</em>
									{:else if !item.release_date || item.release_date > today}
										<em>{year(item.release_date) ? ' · ' : ''}Not out yet</em>
									{/if}
								{:else if item.entry?.status}
									<em class="state" data-status={item.entry.status}
										>{year(item.release_date) ? ' · ' : ''}{statusLabel(
											item.entry.status,
											item.kind
										)}</em
									>
								{/if}
							</span>
						</div>
					</div>
				</div>
			</div>
		{/each}
		{#if bookend}
			<div class="cell last end" aria-hidden="true">
				<div class="book"><span class="bookend"></span></div>
				<div class="ledge" class:sheen>
					<div class="top"></div>
					<div class="front"></div>
				</div>
			</div>
		{/if}
	{/if}
</div>

<style>
	.rows {
		display: grid;
		grid-template-columns: repeat(var(--per), calc(var(--size) + 22px));
		row-gap: 0;
		column-gap: 0;
		min-width: 0;
	}

	.cell {
		display: flex;
		flex-direction: column;
		min-width: 0;
		padding-top: 30px;
	}

	.book {
		position: relative;
		width: var(--size);
		margin: 0 11px;
		animation: deal 560ms var(--spring) calc(min(var(--k), 24) * 22ms) backwards;
	}

	@keyframes deal {
		from {
			opacity: 0;
			transform: translateY(26px) scale(0.94);
		}
	}

	/* A watched title on a watchlist stays, greyed, with a tick. */
	.book.done :global(.art img) {
		filter: grayscale(0.7) brightness(0.55);
	}

	.seen,
	.chip {
		position: absolute;
		z-index: 2;
		pointer-events: none;
	}

	.seen {
		top: 7px;
		right: 7px;
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: 50%;
		background: var(--good);
		color: var(--bg);
		box-shadow: var(--shadow-1);
	}

	.chip {
		left: 7px;
		bottom: 9px;
		padding: 3px 8px;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		background: var(--bg-float);
		font-size: 0.72rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.bookend {
		display: block;
		width: 16px;
		height: calc(var(--size) * 1.05);
		margin-top: calc(var(--size) * 0.45);
		border-radius: 8px 8px 2px 2px;
		background: linear-gradient(90deg, var(--line-strong), var(--bg-elev) 45%, var(--line));
		box-shadow:
			inset 0 1px 0 var(--shine),
			0 10px 20px -8px rgb(0 0 0 / 0.8);
	}

	/* Each cell carries its piece of the shelf; side by side they read as one. */
	.ledge {
		position: relative;
		margin-top: 0;
	}

	.top {
		height: 9px;
		background: linear-gradient(180deg, var(--line-strong), var(--line));
		box-shadow: inset 0 1px 0 var(--shine);
	}

	.front {
		display: flex;
		min-height: 58px;
		padding: 7px 6px 8px;
		background: linear-gradient(180deg, var(--bg-elev-2), var(--bg-elev));
		box-shadow:
			inset 0 1px 0 color-mix(in srgb, var(--bg) 40%, transparent),
			0 26px 30px -18px color-mix(in srgb, var(--bg) 85%, transparent);
	}

	.first .top {
		border-top-left-radius: 5px;
	}

	.first .front {
		border-bottom-left-radius: 10px;
	}

	.last .top {
		border-top-right-radius: 5px;
	}

	.last .front {
		border-bottom-right-radius: 10px;
	}

	.sheen .top {
		background:
			linear-gradient(
					90deg,
					transparent 0 35%,
					color-mix(in srgb, var(--glow, var(--accent)) 70%, transparent) 50%,
					transparent 65% 100%
				)
				bottom / 300% 2px no-repeat,
			linear-gradient(180deg, var(--line-strong), var(--line));
		animation: sheen 4.5s linear infinite;
	}

	@keyframes sheen {
		from {
			background-position:
				100% 100%,
				0 0;
		}
		to {
			background-position:
				-100% 100%,
				0 0;
		}
	}

	/* A label card slotted into the edge under each title. */
	.card {
		flex: 1;
		min-width: 0;
		padding: 5px 9px;
		border-radius: 7px;
		background: color-mix(in srgb, var(--bg) 35%, transparent);
		box-shadow:
			inset 0 1px 3px color-mix(in srgb, var(--bg) 70%, transparent),
			0 1px 0 var(--shine);
		font-size: 0.74rem;
		line-height: 1.3;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.end .front {
		padding: 0;
	}

	.card strong {
		display: block;
		overflow: hidden;
		color: var(--text);
		font-size: 0.78rem;
		font-weight: 650;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card em {
		font-style: normal;
	}

	.card .ok {
		color: var(--good);
	}

	.card .lft {
		color: color-mix(in srgb, var(--glow, var(--accent)) 70%, var(--text));
		font-weight: 600;
	}

	.card .state::before {
		content: '';
	}

	@media (prefers-reduced-motion: reduce) {
		.book,
		.sheen .top {
			animation: none;
		}
	}
</style>
