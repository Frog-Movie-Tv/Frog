<script lang="ts">
	import Icon from './Icon.svelte';
	import { api } from '#lib/api';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { prettyDate } from '#lib/format';
	import { whenWatched } from '#lib/when.svelte';
	import type { Kind, LibraryItem, Status } from '#lib/types';
	import { STATUSES, STATUS_LABEL } from '#lib/types';
	import Rating from './Rating.svelte';
	import Select from './Select.svelte';

	const today = () => new Date().toISOString().slice(0, 10);

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

	/**
	 * Completed means seen: a show marked finished with episodes still open
	 * would count for nothing in the diary, stats or watch time. Ask when, as
	 * ticking them by hand would, and tick every aired one.
	 */
	async function setStatus(status: Status) {
		await save({ status });
		const open = item.total_episodes - item.watched_episodes;
		if (status !== 'completed' || kind !== 'tv' || open <= 0) return;
		// Completed means the seasons; specials are a choice, so offer them.
		const specials = await api
			.season(kind, item.tmdb_id, 0)
			.then((s) =>
				s.episodes
					.filter((e) => !e.watched && (!e.air_date || e.air_date <= today()))
					.map((e) => ({
						key: e.episode_number,
						label: e.name || `Special ${e.episode_number}`,
						hint: e.air_date ? prettyDate(e.air_date) : undefined
					}))
			)
			.catch(() => []);
		const answer = await whenWatched.ask(
			item.name,
			'Every aired episode of every season',
			true,
			specials,
			'Specials you saw too'
		);
		if (!answer) return;
		saving = true;
		try {
			const { item: updated } = await api.setShowWatched(kind, item.tmdb_id, true, answer);
			(onepisodes ?? onchange)(updated);
			toasts.push(`Marked ${updated.watched_episodes} episodes watched`);
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

	.btn.icon {
		width: 42px;
		padding: 10px 0;
	}

	.btn.icon.on {
		color: var(--bad);
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
	}
</style>
