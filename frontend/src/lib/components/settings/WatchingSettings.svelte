<script lang="ts">
	import Rating from '#lib/components/Rating.svelte';
	import Select from '#lib/components/Select.svelte';
	import { SCORE_FORMATS, type ScoreFormat } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { STATUSES, STATUS_LABEL, type Status } from '#lib/types';

	/** Watching: how ratings read, and what tracking does by default. */
	const s = $derived(settings.current);

	/** A rating to try the chosen format on; not saved anywhere. */
	let preview = $state<number | null>(78);
</script>

<section class="block">
	<div class="section-head"><h2>Ratings</h2></div>

	<div class="tiles small">
		{#each SCORE_FORMATS as f (f.id)}
			<button
				class="tile"
				class:on={s.scoreFormat === f.id}
				aria-label={f.label}
				aria-pressed={s.scoreFormat === f.id}
				onclick={() => settings.set('scoreFormat', f.id as ScoreFormat)}
			>
				<strong>{f.label}</strong>
				<span class="faint">{f.example}</span>
			</button>
		{/each}
	</div>

	<div class="rows">
		<label class="row card">
			<span class="label">
				<strong>Half stars</strong>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.halfStars}
				onchange={(e) => settings.set('halfStars', e.currentTarget.checked)}
			/>
		</label>

		<div class="row card">
			<span class="label"><strong>Preview</strong></span>
			<Rating value={preview} onchange={(v) => (preview = v)} />
		</div>
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>Watching</h2></div>

	<div class="rows">
		<div class="row card">
			<span class="label">
				<strong>Default status</strong>
			</span>
			<Select
				label="Default status"
				value={s.defaultStatus}
				options={STATUSES.map((st) => ({ value: st, label: STATUS_LABEL[st] }))}
				onchange={(v) => settings.set('defaultStatus', v as Status)}
			/>
		</div>

		<label class="row card">
			<span class="label">
				<strong>Spoiler protection</strong>
				<span class="faint">Blur unwatched episode stills</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.spoilerProtection}
				onchange={(e) => settings.set('spoilerProtection', e.currentTarget.checked)}
			/>
		</label>

		<label class="row card">
			<span class="label">
				<strong>Hide animation</strong>
				<span class="faint">Leave animated titles out of search</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.hideAnimation}
				onchange={(e) => settings.set('hideAnimation', e.currentTarget.checked)}
			/>
		</label>

		<label class="row card">
			<span class="label">
				<strong>Complete automatically</strong>
				<span class="faint">When the last episode is ticked</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.autoComplete}
				onchange={(e) => settings.set('autoComplete', e.currentTarget.checked)}
			/>
		</label>

		<div class="row card">
			<span class="label"><strong>Dates</strong></span>
			<Select
				label="Dates"
				value={s.dateStyle}
				options={[
					{ value: 'friendly', label: 'Friendly (3 days ago)' },
					{ value: 'long', label: 'Long (24 October 2026)' },
					{ value: 'iso', label: 'ISO (2026-10-24)' }
				]}
				onchange={(v) => settings.set('dateStyle', v as 'friendly')}
			/>
		</div>

		<label class="row card">
			<span class="label"><strong>24-hour clock</strong></span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.clock24}
				onchange={(e) => settings.set('clock24', e.currentTarget.checked)}
			/>
		</label>
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(158px, 1fr));
		gap: 12px;
		margin-bottom: 14px;
	}

	.tiles.small {
		grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 14px;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		cursor: pointer;
		text-align: left;
		transition: border-color var(--fast) var(--ease);
	}

	.tile.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.tile .faint {
		font-size: 0.76rem;
	}

	.rows {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
		cursor: pointer;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.78rem;
	}

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}
	}
</style>
