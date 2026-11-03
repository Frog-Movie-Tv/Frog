<script lang="ts">
	import Icon from './Icon.svelte';
	import { api } from '#lib/api';
	import { completeShow } from '#lib/complete';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUSES, STATUS_LABEL } from '#lib/types';
	import Rating from './Rating.svelte';
	import Select from './Select.svelte';

	interface Props {
		item: LibraryItem;
		onchange: (item: LibraryItem) => void;
		/** Episodes were ticked as well, so lists of them need reloading. */
		onepisodes?: (item: LibraryItem) => void;
	}

	let { item, onchange, onepisodes }: Props = $props();

	const defaultStatus = $derived(settings.current.defaultStatus);

	let saving = $state(false);
	const kind = $derived(item.kind as Kind);
	const tracked = $derived(item.entry !== null);

	async function save(patch: {
		status?: Status;
		rating?: number | null;
		favourite?: boolean;
		notes?: string | null;
	}): Promise<LibraryItem | null> {
		saving = true;
		try {
			const { item: updated } = await api.track(item.id, patch);
			onchange(updated);
			return updated;
		} catch (e) {
			toasts.error(e);
			return null;
		} finally {
			saving = false;
		}
	}

	/**
	 * Completed means seen: a show marked finished with episodes still open
	 * would count for nothing in the diary, stats or watch time. Ask when, as
	 * ticking them by hand would, and tick every aired one. Adding a show with
	 * Completed as the default status goes the same way.
	 */
	async function setStatus(status: Status) {
		const saved = await save({ status });
		if (!saved) return;
		// The answer, not the item from before: a show just added has only now
		// had its episodes listed.
		const open = saved.total_episodes - saved.watched_episodes;
		if (status !== 'completed' || kind !== 'tv' || open <= 0) return;
		saving = true;
		try {
			const updated = await completeShow(saved.id, saved.name);
			if (updated) (onepisodes ?? onchange)(updated);
		} finally {
			saving = false;
		}
	}

	async function remove() {
		// The server says what it took away, so taking it back is exact: every
		// field comes home, the day it was added and a counted progress too.
		const { id, name } = item;
		saving = true;
		try {
			const { before } = await api.bulk({ action: 'remove', title_ids: [id] });
			onchange({ ...item, entry: null });

			toasts.undoable(`Removed ${name}`, async () => {
				try {
					await api.bulk({ action: 'restore', entries: before });
					onchange((await api.title(id)).item);
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
		<button class="btn primary" disabled={saving} onclick={() => setStatus(defaultStatus)}>
			<Icon name="plus" size={16} />
			Add to library
		</button>
		<button class="btn" disabled={saving} onclick={() => save({ status: 'watching' })}>
			Start watching
		</button>
	{:else}
		<Select
			label="Status"
			value={item.entry?.status ?? 'planned'}
			options={STATUSES.map((st) => ({ value: st, label: STATUS_LABEL[st] }))}
			onchange={(v) => setStatus(v as Status)}
		/>

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

		<button
			class="btn icon"
			disabled={saving}
			title="Remove from library"
			aria-label="Remove from library"
			onclick={remove}
		>
			<Icon name="trash" size={16} />
		</button>
	{/if}
</div>

<style>
	.track {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
	}

	.btn.icon {
		width: 42px;
		padding: 0;
	}

	.btn.icon.on {
		color: var(--bad);
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
	}
</style>
