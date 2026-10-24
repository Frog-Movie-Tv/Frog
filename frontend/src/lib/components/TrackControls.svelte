<script lang="ts">
	import Icon from './Icon.svelte';
	import { api } from '#lib/api';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUSES, STATUS_LABEL } from '#lib/types';
	import Rating from './Rating.svelte';

	interface Props {
		item: LibraryItem;
		onchange: (item: LibraryItem) => void;
	}

	let { item, onchange }: Props = $props();

	const defaultStatus = $derived(settings.current.defaultStatus);

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
		// Hold on to what was there, so taking it back is exact rather than
		// approximate: status, rating, favourite and notes all come home.
		const previous = item.entry;
		saving = true;
		try {
			await api.untrack(kind, item.tmdb_id);
			onchange({ ...item, entry: null });

			toasts.undoable(`Removed ${item.name}`, async () => {
				if (!previous) return;
				try {
					const { item: restored } = await api.track(kind, item.tmdb_id, {
						status: previous.status,
						rating: previous.rating,
						favourite: previous.favourite,
						notes: previous.notes
					});
					onchange(restored);
				} catch (e) {
					toasts.error(e);
				}
			});
		} catch (e) {
			toasts.error(e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="track">
	{#if !tracked}
		<button class="btn primary" disabled={saving} onclick={() => save({ status: defaultStatus })}>
			<Icon name="plus" size={16} />
			Add to library
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

		<Rating value={item.entry?.rating ?? null} onchange={(v) => save({ rating: v })} />

		<button
			class="btn icon"
			class:on={item.entry?.favourite}
			disabled={saving}
			aria-pressed={item.entry?.favourite}
			title="Favourite"
			aria-label="Favourite"
			onclick={() => save({ favourite: !item.entry?.favourite })}
		>
			<Icon name="heart" filled={item.entry?.favourite} size={17} />
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

	.btn.icon {
		width: 42px;
		padding: 10px 0;
	}

	.btn.icon.on {
		color: var(--bad);
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
	}
</style>
