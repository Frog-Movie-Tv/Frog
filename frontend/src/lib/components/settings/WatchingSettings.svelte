<script lang="ts">
	import Rating from '#lib/components/Rating.svelte';
	import Select from '#lib/components/Select.svelte';
	import { SCORE_FORMATS, type ScoreFormat } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { STATUS_LABEL, STATUSES, type Status, statusLabel } from '#lib/types';

	/** Watching: how ratings read, and what tracking does by default. */
	const s = $derived(settings.current);

	/** A rating to try the chosen format on; not saved anywhere. */
	let preview = $state<number | null>(78);

	/** A status's colour as the page draws it now, for the picker to start on. */
	function colourOf(status: Status): string {
		const own = s.statusColors?.[status];
		if (own) return own;
		const drawn = getComputedStyle(document.documentElement)
			.getPropertyValue(`--status-${status}`)
			.trim();
		return /^#[0-9a-f]{6}$/i.test(drawn) ? drawn : '#888888';
	}

	function rename(status: Status, name: string) {
		const names = { ...s.statusNames };
		const trimmed = name.trim().slice(0, 24);
		if (trimmed && trimmed !== STATUS_LABEL[status]) names[status] = trimmed;
		else delete names[status];
		settings.set('statusNames', names);
	}

	function recolour(status: Status, colour: string | null) {
		const colours = { ...s.statusColors };
		if (colour) colours[status] = colour;
		else delete colours[status];
		settings.set('statusColors', colours);
	}
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

	<div class="rows list-pane">
		<label class="row">
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

		<div class="row">
			<span class="label"><strong>Preview</strong></span>
			<Rating value={preview} onchange={(v) => (preview = v)} />
		</div>
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>Watching</h2></div>

	<div class="rows list-pane">
		<div class="row">
			<span class="label">
				<strong>Default status</strong>
			</span>
			<Select
				label="Default status"
				value={s.defaultStatus}
				options={STATUSES.map((st) => ({ value: st, label: statusLabel(st) }))}
				onchange={(v) => settings.set('defaultStatus', v as Status)}
			/>
		</div>

		<label class="row">
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

		<label class="row">
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

		<label class="row">
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

		<div class="row">
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

		<label class="row">
			<span class="label"><strong>24-hour clock</strong></span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.clock24}
				onchange={(e) => settings.set('clock24', e.currentTarget.checked)}
			/>
		</label>

		<label class="row">
			<span class="label">
				<strong>Play trailers</strong>
				<span class="faint">Without sound, behind a title's page</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.trailerAutoplay ?? false}
				onchange={(e) => settings.set('trailerAutoplay', e.currentTarget.checked)}
			/>
		</label>
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>Statuses</h2></div>
	<p class="faint lead">Call a status what you like and give it its own colour.</p>

	<div class="rows list-pane">
		{#each STATUSES as st (st)}
			{@const own = s.statusNames?.[st] ?? ''}
			<div class="row status-row">
				<span class="swatch" data-status={st} aria-hidden="true"></span>
				<input
					class="input name"
					value={own}
					maxlength="24"
					placeholder={STATUS_LABEL[st]}
					aria-label="Name for {STATUS_LABEL[st]}"
					onchange={(e) => rename(st, e.currentTarget.value)}
				/>
				<input
					type="color"
					class="colour"
					value={colourOf(st)}
					aria-label="Colour for {statusLabel(st)}"
					onchange={(e) => recolour(st, e.currentTarget.value)}
				/>
				{#if own || s.statusColors?.[st]}
					<button
						class="btn ghost small"
						onclick={() => {
							rename(st, '');
							recolour(st, null);
						}}>Reset</button
					>
				{/if}
			</div>
		{/each}
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
	}

	/* Score formats: one pane of choices, the chosen one a raised capsule
	   ringed in the accent, as in a segmented control. */
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
		gap: 6px;
		margin-bottom: 14px;
		padding: 8px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-2);
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 10px 12px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: inherit;
		cursor: pointer;
		text-align: left;
		transition: background var(--fast) var(--ease);
	}

	.tile:hover {
		background: var(--bg-hover);
	}

	.tile.on {
		background: var(--seg-on);
		box-shadow: inset 0 0 0 1.5px var(--accent);
	}

	.tile .faint {
		font-size: 0.76rem;
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

	.lead {
		margin: -4px 0 14px;
		font-size: 0.86rem;
	}

	.status-row {
		justify-content: flex-start;
		gap: 12px;
		cursor: default;
	}

	.swatch {
		flex: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--status);
	}

	.status-row .name {
		flex: 1;
		min-width: 0;
		max-width: 260px;
	}

	.colour {
		flex: none;
		width: 40px;
		height: 32px;
		padding: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: none;
		cursor: pointer;
	}

	@media (max-width: 720px) {
		.row:not(.status-row) {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}
	}
</style>
