<script lang="ts">
	import { episodeCode, formatScore } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { HeatmapCell } from '#lib/types';

	interface Props {
		cells: HeatmapCell[];
	}

	let { cells }: Props = $props();

	/** Group into seasons, keeping episode order, so each row is one season. */
	const seasons = $derived.by(() => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const map = new Map<number, HeatmapCell[]>();
		for (const cell of cells) {
			const list = map.get(cell.s) ?? [];
			list.push(cell);
			map.set(cell.s, list);
		}
		return [...map.entries()]
			.sort((a, b) => a[0] - b[0])
			.map(([season, eps]) => ({ season, eps: eps.sort((a, b) => a.e - b.e) }));
	});

	const widest = $derived(Math.max(1, ...seasons.map((s) => s.eps.length)));
	const mine = $derived(cells.filter((c) => c.rating !== null).length);
	const theirs = $derived(cells.filter((c) => c.tmdb !== null).length);

	/**
	 * Every episode is filled with TMDB's score, so a season has a shape from
	 * the moment the show is added. Your own rating paints over the top at
	 * full strength and gets a ring, so yours always wins the eye.
	 */
	function fill(cell: HeatmapCell): string {
		const score = cell.rating ?? cell.tmdb;
		if (score === null || score === undefined) {
			return cell.watched ? 'var(--line-strong)' : 'var(--bg-elev-2)';
		}
		const base = `color-mix(in oklab, var(--bad), var(--good) ${Math.round(score)}%)`;
		return cell.rating === null ? `color-mix(in oklab, ${base} 55%, var(--bg-elev))` : base;
	}

	function label(cell: HeatmapCell): string {
		const fmt = settings.current.scoreFormat;
		const parts = [episodeCode(cell.s, cell.e)];
		if (cell.rating !== null) parts.push(`you: ${formatScore(cell.rating, fmt)}`);
		if (cell.tmdb !== null && cell.tmdb !== undefined) {
			parts.push(`TMDB: ${(cell.tmdb / 10).toFixed(1)}`);
		}
		if (!cell.watched) parts.push('unwatched');
		return parts.join(' · ');
	}
</script>

{#if seasons.length}
	<div class="heatmap" style:--cols={widest}>
		{#each seasons as row (row.season)}
			<div class="row">
				<span class="label faint">S{row.season}</span>
				<div class="cells">
					{#each row.eps as cell (cell.e)}
						<span
							class="cell"
							class:mine={cell.rating !== null}
							class:unwatched={!cell.watched}
							style:background={fill(cell)}
							title={label(cell)}
							aria-label={label(cell)}
						></span>
					{/each}
				</div>
			</div>
		{/each}
	</div>

	<p class="legend faint">
		<span class="key"><i class="sample mine"></i> your rating ({mine})</span>
		<span class="key"><i class="sample"></i> TMDB ({theirs})</span>
		<span class="scale" aria-hidden="true">
			<i style:background="var(--bad)"></i>
			<i style:background="color-mix(in oklab, var(--bad), var(--good) 50%)"></i>
			<i style:background="var(--good)"></i>
		</span>
	</p>
{/if}

<style>
	.heatmap {
		display: flex;
		flex-direction: column;
		gap: 5px;
		overflow-x: auto;
		padding-bottom: 6px;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.label {
		width: 2.8ch;
		font-size: 0.76rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
		flex: none;
	}

	.cells {
		display: grid;
		grid-template-columns: repeat(var(--cols), minmax(16px, 26px));
		gap: 5px;
	}

	.cell {
		aspect-ratio: 1;
		border-radius: 5px;
		border: 1.5px solid transparent;
	}

	.cell.mine {
		border-color: color-mix(in srgb, var(--text) 45%, transparent);
	}

	.cell.unwatched {
		opacity: 0.6;
	}

	.legend {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 16px;
		margin-top: 12px;
		font-size: 0.76rem;
	}

	.key {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	.sample {
		width: 13px;
		height: 13px;
		border-radius: 4px;
		border: 1.5px solid transparent;
		background: color-mix(
			in oklab,
			color-mix(in oklab, var(--bad), var(--good) 70%) 55%,
			var(--bg-elev)
		);
	}

	.sample.mine {
		background: color-mix(in oklab, var(--bad), var(--good) 70%);
		border-color: color-mix(in srgb, var(--text) 45%, transparent);
	}

	.scale {
		display: inline-flex;
		gap: 3px;
		margin-left: auto;
	}

	.scale i {
		width: 16px;
		height: 9px;
		border-radius: 3px;
	}
</style>
