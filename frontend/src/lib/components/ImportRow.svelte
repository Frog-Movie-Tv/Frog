<script lang="ts">
	import { img } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import type { ReviewRow } from '#lib/import/review';
	import type { SearchItem, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	interface Props {
		row: ReviewRow;
		/** Under its show's heading, where the name and poster would only repeat. */
		grouped?: boolean;
		oninclude: (row: ReviewRow, include: boolean) => void;
		onchoose: (row: ReviewRow, candidate: SearchItem) => void;
		onseason: (row: ReviewRow, value: string) => void;
		onstatus: (row: ReviewRow, status: Status) => void;
	}

	let { row, grouped = false, oninclude, onchoose, onseason, onstatus }: Props = $props();

	const label = $derived(row.incoming.label ?? row.incoming.title);
</script>

<div class="row" class:off={!row.include} class:unmatched={!row.chosen} class:grouped>
	<input
		type="checkbox"
		checked={row.include}
		disabled={!row.chosen}
		aria-label="Include {label}"
		onchange={(e) => oninclude(row, e.currentTarget.checked)}
	/>

	{#if grouped}
		<!-- The season is the row here; the heading above says which show. -->
	{:else if row.chosen?.poster_path}
		<img src={img(row.chosen.poster_path, 'w92')} alt="" loading="lazy" />
	{:else}
		<span class="no-art"><Icon name="film" size={15} /></span>
	{/if}

	<div class="what">
		{#if !grouped}
			<div class="line">
				<strong>{row.chosen?.name ?? row.incoming.title}</strong>
				{#if row.chosen?.year}<span class="faint">{row.chosen.year}</span>{/if}
				{#if row.chosen}
					<span class="pill tiny">{row.chosen.kind === 'tv' ? 'Show' : 'Film'}</span>
				{/if}
			</div>
		{/if}
		<div class="sub faint">
			<span>{label}{row.incoming.year ? ` (${row.incoming.year})` : ''}</span>
			{#if row.incoming.progress}<span>{row.incoming.progress} watched</span>{/if}
			{#if row.incoming.rating !== null}<span>rated {Math.round(row.incoming.rating)}</span>{/if}
			{#if row.match?.already_tracked}
				<span class="already">already in library</span>
			{/if}
		</div>

		{#if row.incoming.warning}
			<p class="note-line">{row.incoming.warning}</p>
		{/if}
		{#if row.incoming.info}
			<p class="info-line">{row.incoming.info}</p>
		{/if}
		{#if row.match?.note}
			<p class="note-line">{row.match.note}</p>
		{/if}

		{#if row.match?.candidates?.length}
			<div class="candidates">
				{#each row.match.candidates as c (`${c.kind}-${c.tmdb_id}`)}
					<button class="cand" onclick={() => onchoose(row, c)}>
						{c.name}{c.year ? ` (${c.year})` : ''}
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="row-controls">
		{#if row.chosen?.kind === 'tv' && (row.incoming.anime || row.incoming.netflix)}
			<label class="season" title="TMDB season">
				<span>S</span>
				<input
					type="number"
					min="0"
					inputmode="numeric"
					aria-label="Season for {label}"
					value={row.incoming.season ?? ''}
					placeholder="?"
					onchange={(e) => onseason(row, e.currentTarget.value)}
				/>
			</label>
		{/if}
		<Select
			value={row.incoming.status ?? 'completed'}
			label="Status"
			compact
			options={STATUSES.map((s) => ({ value: s, label: STATUS_LABEL[s] }))}
			onchange={(v) => onstatus(row, v as Status)}
		/>
	</div>
</div>

<style>
	.row {
		display: grid;
		grid-template-columns: auto 40px minmax(0, 1fr) auto;
		align-items: center;
		gap: 14px;
		padding: 10px 14px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.row.grouped {
		grid-template-columns: auto minmax(0, 1fr) auto;
		padding: 8px 14px;
		border: none;
		border-radius: 0;
		border-top: 1px solid var(--line);
		background: none;
	}

	.row.off {
		opacity: 0.45;
	}

	.row.unmatched {
		border-color: color-mix(in srgb, var(--warn) 45%, transparent);
	}

	.row img,
	.no-art {
		width: 40px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-faint);
		object-fit: cover;
	}

	input[type='checkbox'] {
		width: 18px;
		height: 18px;
		accent-color: var(--accent);
	}

	.what {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.line strong {
		font-size: 0.92rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.sub {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		font-size: 0.75rem;
	}

	.grouped .sub {
		font-size: 0.82rem;
	}

	.already {
		color: var(--accent);
	}

	.note-line {
		font-size: 0.75rem;
		color: var(--warn);
	}

	.info-line {
		font-size: 0.75rem;
		color: var(--text-faint);
	}

	.candidates {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 4px;
	}

	.cand {
		padding: 3px 9px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line-strong);
		background: transparent;
		color: var(--text-dim);
		font-size: 0.74rem;
		cursor: pointer;
	}

	.cand:hover {
		color: var(--text);
		border-color: var(--accent);
	}

	.pill.tiny {
		padding: 1px 7px;
		font-size: 0.66rem;
	}

	.row-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.season {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px 0 9px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		font-size: 0.78rem;
		color: var(--text-faint);
	}

	.season input {
		width: 3.2ch;
		padding: 6px 2px;
		border: none;
		background: none;
		font-size: 0.82rem;
		font-variant-numeric: tabular-nums;
		color: var(--text);
		-moz-appearance: textfield;
		appearance: textfield;
	}

	.season input::-webkit-inner-spin-button {
		display: none;
	}

	@media (max-width: 760px) {
		.row {
			grid-template-columns: auto 40px minmax(0, 1fr);
			row-gap: 8px;
		}

		.row-controls {
			grid-column: 2 / -1;
		}

		.row.grouped {
			grid-template-columns: auto minmax(0, 1fr);
		}
	}
</style>
