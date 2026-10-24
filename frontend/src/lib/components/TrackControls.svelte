<script lang="ts">
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUSES, STATUS_LABEL } from '#lib/types';
	import ScoreInput from './ScoreInput.svelte';

	interface Props {
		item: LibraryItem;
		onchange: (item: LibraryItem) => void;
	}

	let { item, onchange }: Props = $props();

	let saving = $state(false);
	const kind = $derived(item.kind as Kind);
	const tracked = $derived(item.entry !== null);

	async function save(patch: {
		status?: Status;
		rating?: number | null;
		favourite?: boolean;
		notes?: string | null;
	}) {
		saving = true;
		try {
			const { item: updated } = await api.track(kind, item.tmdb_id, patch);
			onchange(updated);
		} catch (e) {
			toasts.error(e);
		} finally {
			saving = false;
		}
	}

	async function remove() {
		saving = true;
		try {
			await api.untrack(kind, item.tmdb_id);
			onchange({ ...item, entry: null });
			toasts.push('Removed from your library');
		} catch (e) {
			toasts.error(e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="track">
	{#if !tracked}
		<button class="btn primary" disabled={saving} onclick={() => save({ status: 'planned' })}>
			＋ Add to library
		</button>
		<button class="btn" disabled={saving} onclick={() => save({ status: 'watching' })}>
			Start watching
		</button>
	{:else}
		<label class="status">
			<select
				class="input"
				value={item.entry?.status}
				disabled={saving}
				onchange={(e) => save({ status: e.currentTarget.value as Status })}
			>
				{#each STATUSES as s (s)}
					<option value={s}>{STATUS_LABEL[s]}</option>
				{/each}
			</select>
		</label>

		<div class="rating">
			<ScoreInput value={item.entry?.rating ?? null} onchange={(v) => save({ rating: v })} />
		</div>

		<button
			class="btn icon"
			class:on={item.entry?.favourite}
			disabled={saving}
			aria-pressed={item.entry?.favourite}
			title="Favourite"
			aria-label="Favourite"
			onclick={() => save({ favourite: !item.entry?.favourite })}
		>
			{item.entry?.favourite ? '♥' : '♡'}
		</button>

		<button class="btn ghost" disabled={saving} onclick={remove}>Remove</button>
	{/if}
</div>

<style>
	.track {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
	}

	.status select {
		padding: 10px 14px;
		border-radius: var(--radius-pill);
		border-color: var(--line-strong);
		background: var(--bg-elev-2);
		font-weight: 600;
		font-size: 0.9rem;
		cursor: pointer;
	}

	.rating {
		min-width: 190px;
		padding: 6px 14px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: color-mix(in srgb, var(--bg-elev-2) 70%, transparent);
	}

	.btn.icon {
		width: 42px;
		padding: 10px 0;
		font-size: 1.05rem;
	}

	.btn.icon.on {
		color: var(--bad);
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
	}
</style>
