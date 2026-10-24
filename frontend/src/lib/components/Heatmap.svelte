<script lang="ts">
	import { formatScore, episodeCode } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { HeatmapCell } from '#lib/types';

	interface Props {
		cells: HeatmapCell[];
	}

	let { cells }: Props = $props();

	/** Group into seasons, keeping episode order, so each row is one season. */
	const seasons = $derived.by(() => {
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
	const rated = $derived(cells.filter((c) => c.rating !== null).length);

	function tint(cell: HeatmapCell): string {
		if (cell.rating === null) {
			return cell.watched ? 'var(--line-strong)' : 'var(--bg-elev-2)';
		}
		// Red through amber to green across the rating range.
		return `color-mix(in oklab, var(--bad), var(--good) ${Math.round(cell.rating)}%)`;
	}

	function label(cell: HeatmapCell): string {
		const score =
			cell.rating === null ? 'not rated' : formatScore(cell.rating, settings.current.scoreFormat);
		return `${episodeCode(cell.s, cell.e)} — ${score}${cell.watched ? '' : ', unwatched'}`;
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
							class:unwatched={!cell.watched}
							style:background={tint(cell)}
							title={label(cell)}
							aria-label={label(cell)}
						></span>
					{/each}
				</div>
			</div>
		{/each}
	</div>

	<p class="legend faint">
		{rated} of {cells.length} episodes rated
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
		gap: 4px;
		overflow-x: auto;
		padding-bottom: 6px;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.label {
		width: 2.6ch;
		font-size: 0.7rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
		flex: none;
	}

	.cells {
		display: grid;
		grid-template-columns: repeat(var(--cols), minmax(9px, 15px));
		gap: 3px;
	}

	.cell {
		aspect-ratio: 1;
		border-radius: 3px;
		border: 1px solid transparent;
	}

	.cell.unwatched {
		border-color: var(--line);
		opacity: 0.55;
	}

	.legend {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 10px;
		font-size: 0.74rem;
	}

	.scale {
		display: inline-flex;
		gap: 2px;
	}

	.scale i {
		width: 14px;
		height: 8px;
		border-radius: 2px;
	}
</style>
