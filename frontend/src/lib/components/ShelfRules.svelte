<script lang="ts">
	import Select from './Select.svelte';
	import { api } from '#lib/api';
	import { formatScore } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import type { ShelfRules, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	interface Props {
		rules: ShelfRules;
		onsave: (rules: ShelfRules) => void;
		oncancel: () => void;
	}

	let { rules, onsave, oncancel }: Props = $props();

	// Edit a copy of the rules as they were when the editor opened; nothing
	// changes until Save.
	// svelte-ignore state_referenced_locally
	let draft = $state<ShelfRules>(structuredClone($state.snapshot(rules)));
	let genres = $state<string[]>([]);

	// Only the genres actually in the library are worth offering.
	$effect(() => {
		void api
			.library({ limit: 2000 })
			.then(({ items }) => {
				const all = new Set(items.flatMap((i) => i.genres));
				for (const g of draft.genres ?? []) all.add(g);
				genres = [...all].sort((a, b) => a.localeCompare(b));
			})
			.catch(() => (genres = draft.genres ?? []));
	});

	function toggle<T>(list: T[] | undefined, value: T): T[] {
		const current = list ?? [];
		return current.includes(value) ? current.filter((v) => v !== value) : [...current, value];
	}

	const ANY = 'any';
	const scores = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
	const scoreOptions = $derived([
		{ value: ANY, label: 'Any' },
		...scores.map((v) => ({ value: String(v), label: formatScore(v, settings.current.scoreFormat) }))
	]);

	const num = (v: string) => (v === ANY ? null : Number(v));
	const str = (v: number | string | null | undefined) => (v === null || v === undefined ? ANY : String(v));

	function year(value: string): number | null {
		const n = Number(value);
		return value.trim() && Number.isInteger(n) && n > 1800 && n < 2200 ? n : null;
	}

	function save() {
		// Drop empties so the stored rules read the way they look.
		const clean: ShelfRules = {};
		if (draft.kind) clean.kind = draft.kind;
		if (draft.statuses?.length) clean.statuses = draft.statuses;
		if (draft.genres?.length) clean.genres = draft.genres;
		if (draft.min_rating != null) clean.min_rating = draft.min_rating;
		if (draft.max_rating != null) clean.max_rating = draft.max_rating;
		if (draft.progress) clean.progress = draft.progress;
		if (draft.favourite) clean.favourite = true;
		if (draft.from_year != null) clean.from_year = draft.from_year;
		if (draft.to_year != null) clean.to_year = draft.to_year;
		if (draft.sort) clean.sort = draft.sort;
		if (draft.order) clean.order = draft.order;
		if (draft.limit) clean.limit = draft.limit;
		onsave(clean);
	}
</script>

<section class="rules card" aria-label="Rules">
	<div class="grid">
		<div class="field">
			<span>Type</span>
			<Select
				label="Type"
				full
				value={str(draft.kind)}
				options={[
					{ value: ANY, label: 'Films and shows' },
					{ value: 'movie', label: 'Films' },
					{ value: 'tv', label: 'Shows' }
				]}
				onchange={(v) => (draft.kind = v === ANY ? null : (v as 'movie' | 'tv'))}
			/>
		</div>

		<div class="field">
			<span>Progress</span>
			<Select
				label="Progress"
				full
				value={str(draft.progress)}
				options={[
					{ value: ANY, label: 'Any' },
					{ value: 'unstarted', label: 'Not started' },
					{ value: 'started', label: 'In progress', hint: 'Shows part-way through' },
					{ value: 'unfinished', label: 'Not finished' },
					{ value: 'finished', label: 'Finished' }
				]}
				onchange={(v) => (draft.progress = v === ANY ? null : (v as ShelfRules['progress']))}
			/>
		</div>

		<div class="field">
			<span>Rated at least</span>
			<Select label="Rated at least" full value={str(draft.min_rating)} options={scoreOptions} onchange={(v) => (draft.min_rating = num(v))} />
		</div>

		<div class="field">
			<span>Rated at most</span>
			<Select label="Rated at most" full value={str(draft.max_rating)} options={scoreOptions} onchange={(v) => (draft.max_rating = num(v))} />
		</div>

		<label class="field">
			<span>Released from</span>
			<input
				class="input"
				inputmode="numeric"
				placeholder="Any year"
				value={draft.from_year ?? ''}
				oninput={(e) => (draft.from_year = year(e.currentTarget.value))}
			/>
		</label>

		<label class="field">
			<span>Released until</span>
			<input
				class="input"
				inputmode="numeric"
				placeholder="Any year"
				value={draft.to_year ?? ''}
				oninput={(e) => (draft.to_year = year(e.currentTarget.value))}
			/>
		</label>

		<div class="field">
			<span>Sort by</span>
			<Select
				label="Sort by"
				full
				value={draft.sort ?? 'added'}
				options={[
					{ value: 'added', label: 'Date added' },
					{ value: 'updated', label: 'Last changed' },
					{ value: 'last_watched', label: 'Last watched' },
					{ value: 'rating', label: 'Rating' },
					{ value: 'release', label: 'Release date' },
					{ value: 'name', label: 'Name' }
				]}
				onchange={(v) => {
					draft.sort = v as ShelfRules['sort'];
					// A to Z for names; newest or highest first for everything else.
					draft.order = v === 'name' ? 'asc' : 'desc';
				}}
			/>
		</div>

		<div class="field">
			<span>Show</span>
			<Select
				label="Show"
				full
				value={str(draft.limit)}
				options={[
					{ value: ANY, label: 'Everything' },
					{ value: '10', label: 'Top 10' },
					{ value: '20', label: 'Top 20' },
					{ value: '50', label: 'Top 50' },
					{ value: '100', label: 'Top 100' }
				]}
				onchange={(v) => (draft.limit = num(v))}
			/>
		</div>
	</div>

	<div class="field">
		<span>Status</span>
		<div class="chips" role="group" aria-label="Status">
			{#each STATUSES as s (s)}
				<button
					type="button"
					class="chip"
					data-status={s}
					class:on={draft.statuses?.includes(s as Status)}
					aria-pressed={!!draft.statuses?.includes(s as Status)}
					onclick={() => (draft.statuses = toggle(draft.statuses, s as Status))}
				>
					{STATUS_LABEL[s]}
				</button>
			{/each}
		</div>
	</div>

	{#if genres.length}
		<div class="field">
			<span>Genres</span>
			<div class="chips" role="group" aria-label="Genres">
				{#each genres as g (g)}
					<button
						type="button"
						class="chip"
						class:on={draft.genres?.includes(g)}
						aria-pressed={!!draft.genres?.includes(g)}
						onclick={() => (draft.genres = toggle(draft.genres, g))}
					>
						{g}
					</button>
				{/each}
			</div>
		</div>
	{/if}

	<label class="check">
		<input type="checkbox" checked={!!draft.favourite} onchange={(e) => (draft.favourite = e.currentTarget.checked)} />
		Favourites only
	</label>

	<div class="actions">
		<button class="btn ghost" onclick={oncancel}>Cancel</button>
		<button class="btn primary" onclick={save}>Save rules</button>
	</div>
</section>

<style>
	.rules {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 18px;
		margin-bottom: 24px;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 200px), 1fr));
		gap: 14px;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.chip {
		padding: 6px 12px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.82rem;
		font-weight: 600;
		cursor: pointer;
	}

	.chip.on {
		border-color: var(--status, var(--accent));
		color: var(--text);
		box-shadow: 0 0 0 1px var(--status, var(--accent)) inset;
	}

	.check {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.9rem;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
	}
</style>
