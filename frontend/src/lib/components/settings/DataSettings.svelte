<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import { settings } from '#lib/settings.svelte';

	/** Your data: bringing a library in, taking copies of it out. */
	const s = $derived(settings.current);
</script>

<section class="block">
	<div class="section-head"><h2>Your data</h2></div>

	<div class="rows">
		<a class="row card link-row" href="/import">
			<span class="label">
				<strong>Import</strong>
				<span class="faint">Letterboxd, Trakt, IMDb, MyAnimeList, TV Time or a Frog backup</span>
			</span>
			<Icon name="chevron-right" size={18} />
		</a>
		<a class="row card link-row" href="/api/export/library.json" download>
			<span class="label">
				<strong>Download backup</strong>
				<span class="faint">Everything, as JSON</span>
			</span>
			<Icon name="download" size={18} />
		</a>
		<a class="row card link-row" href="/api/export/library.csv" download>
			<span class="label">
				<strong>Download spreadsheet</strong>
				<span class="faint">Your library, as CSV</span>
			</span>
			<Icon name="download" size={18} />
		</a>
		<label class="row card">
			<span class="label">
				<strong>Data saver</strong>
				<span class="faint">Smaller artwork everywhere</span>
			</span>
			<input
				type="checkbox"
				class="toggle-switch"
				checked={s.dataSaver}
				onchange={(e) => settings.set('dataSaver', e.currentTarget.checked)}
			/>
		</label>
	</div>
</section>

<style>
	.block {
		margin-bottom: 44px;
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

	.link-row {
		color: inherit;
	}

	.link-row:hover {
		border-color: var(--line-strong);
	}

	@media (max-width: 720px) {
		.row {
			align-items: flex-start;
			flex-direction: column;
			gap: 10px;
		}
	}
</style>
