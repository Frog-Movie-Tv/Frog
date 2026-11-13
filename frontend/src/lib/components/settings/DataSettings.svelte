<script lang="ts">
	import { api } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import { prettyDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { DEFAULTS, settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { TitleMerge } from '#lib/types';

	/** Your data: bringing a library in, taking copies of it out. */
	const s = $derived(settings.current);

	function resetSettings() {
		const before = s;
		// Which release you have seen and whether you were welcomed are history,
		// not preferences; resetting them would bring both back.
		settings.replace({
			...DEFAULTS,
			lastSeenVersion: before.lastSeenVersion,
			onboarded: before.onboarded,
			homeRowsSeen: before.homeRowsSeen
		});
		toasts.undoable('Settings reset', () => settings.replace(before));
	}

	/** Titles Frog folded into another, for taking back. Shown on request. */
	let merges = $state<TitleMerge[]>([]);
	let showMerges = $state(false);
	let undoing = $state<number | null>(null);

	$effect(() => {
		if (!session.authenticated) return;
		api
			.merges()
			.then((answer) => (merges = answer.merges))
			.catch(() => (merges = []));
	});

	function why(reason: string): string {
		if (reason.startsWith('anilist')) return 'Matched from AniList';
		if (reason.includes('same film')) return 'The same film';
		if (reason.includes('same show')) return 'The same show';
		return 'Merged';
	}

	async function undo(merge: TitleMerge) {
		undoing = merge.id;
		try {
			await api.undoMerge(merge.id);
			merges = (await api.merges()).merges;
			toasts.push(`${merge.from_name} is its own title again`);
		} catch (e) {
			toasts.error(e);
		} finally {
			undoing = null;
		}
	}
</script>

<section class="block">
	<div class="section-head"><h2>Your data</h2></div>

	<div class="rows list-pane">
		<a class="row link-row" href="/import">
			<span class="label">
				<strong>Import</strong>
				<span class="faint">From another tracker or a Frog backup</span>
			</span>
			<Icon name="chevron-right" size={18} />
		</a>

		<div class="row">
			<span class="label">
				<strong>Download a copy</strong>
				<span class="faint">Everything you track</span>
			</span>
			<span class="buttons">
				<a class="btn ghost small" href="/api/export/library.json" download>
					<Icon name="download" size={14} /> Backup
				</a>
				<a class="btn ghost small" href="/api/export/library.csv" download>
					<Icon name="download" size={14} /> Spreadsheet
				</a>
			</span>
		</div>

		{#if merges.length}
			<div class="row">
				<span class="label">
					<strong>Merged titles</strong>
					<span class="faint">Titles Frog found to be one</span>
				</span>
				<button
					class="btn ghost small"
					aria-expanded={showMerges}
					onclick={() => (showMerges = !showMerges)}
				>
					{showMerges ? 'Hide' : `Show (${merges.length})`}
				</button>
			</div>
			{#if showMerges}
				<ul class="merges">
					{#each merges as merge (merge.id)}
						<li>
							<span class="label">
								<strong
									><a href="/t/{merge.from_id}">{merge.from_name}</a> →
									<a href="/t/{merge.to_id}">{merge.to_name}</a></strong
								>
								<span class="faint">{why(merge.reason)} · {prettyDate(merge.merged_at)}</span>
							</span>
							{#if merge.undone_at}
								<span class="faint undone">Undone</span>
							{:else}
								<button
									class="btn ghost small"
									disabled={undoing !== null}
									onclick={() => undo(merge)}
								>
									{undoing === merge.id ? 'Undoing…' : 'Undo'}
								</button>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</div>
</section>

<section class="block">
	<div class="section-head"><h2>This device</h2></div>

	<div class="rows list-pane">
		<label class="row">
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
		<div class="row">
			<span class="label">
				<strong>Reset settings</strong>
				<span class="faint">Every setting back as it came</span>
			</span>
			<button class="btn ghost small" aria-label="Reset all settings" onclick={resetSettings}
				>Reset</button
			>
		</div>
	</div>
</section>

<style>
	.block {
		margin-bottom: 36px;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
		cursor: pointer;
	}

	div.row {
		cursor: default;
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

	.buttons {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 6px;
	}

	.btn.small {
		flex: none;
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	/* The merges, opened under their row, inside the same pane. */
	.merges {
		list-style: none;
		margin: 0;
		padding: 4px 18px 10px;
		background: var(--bg-elev-2);
	}

	.merges li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 10px 0;
	}

	.merges li + li {
		border-top: 1px solid var(--line);
	}

	.merges strong {
		font-size: 0.86rem;
		font-weight: 600;
	}

	.merges a:hover {
		text-decoration: underline;
	}

	.undone {
		flex: none;
		font-size: 0.8rem;
	}

	@media (max-width: 600px) {
		.row {
			flex-wrap: wrap;
		}
	}
</style>
