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

	/** Titles Frog folded into another, for taking back. */
	let merges = $state<TitleMerge[]>([]);
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
				<span class="faint">
					Letterboxd, Trakt, IMDb, MyAnimeList, TV Time, Netflix or a Frog backup
				</span>
			</span>
			<Icon name="chevron-right" size={18} />
		</a>
		<a class="row link-row" href="/api/export/library.json" download>
			<span class="label">
				<strong>Download backup</strong>
				<span class="faint">Everything, as JSON</span>
			</span>
			<Icon name="download" size={18} />
		</a>
		<a class="row link-row" href="/api/export/library.csv" download>
			<span class="label">
				<strong>Download spreadsheet</strong>
				<span class="faint">Your library, as CSV</span>
			</span>
			<Icon name="download" size={18} />
		</a>
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
	</div>
	<div class="reset">
		<button class="btn ghost" onclick={resetSettings}>Reset all settings</button>
	</div>
</section>

{#if merges.length}
	<section class="block">
		<div class="section-head"><h2>Merged titles</h2></div>
		<p class="faint lead">
			When two titles turn out to be one, your history moves onto the one Frog keeps. Undo puts it
			back as it was.
		</p>
		<div class="rows list-pane">
			{#each merges as merge (merge.id)}
				<div class="row merge">
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
						<button class="btn ghost small" disabled={undoing !== null} onclick={() => undo(merge)}>
							{undoing === merge.id ? 'Undoing…' : 'Undo'}
						</button>
					{/if}
				</div>
			{/each}
		</div>
	</section>
{/if}

<style>
	.reset {
		margin-top: 14px;
	}

	.block {
		margin-bottom: 44px;
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

	.lead {
		margin: -6px 0 14px;
		font-size: 0.86rem;
	}

	.merge {
		cursor: default;
	}

	.merge a:hover {
		text-decoration: underline;
	}

	.btn.small {
		flex: none;
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.undone {
		flex: none;
		font-size: 0.8rem;
	}
</style>
