<script lang="ts">
	import { api } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import NewShelf from '#lib/components/NewShelf.svelte';
	import { plural } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import { whenWatched, type WhenAnswer } from '#lib/when.svelte';
	import type { EntryRecord, SharedItem, Shelf, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	interface Props {
		/** The titles ticked, among those on screen. */
		chosen: SharedItem[];
		/** How many titles the filters show. */
		shown: number;
		onselectall: () => void;
		ondone: () => void;
		/** Something changed: read the list again, and start a fresh selection. */
		onchanged: () => void | Promise<void>;
	}

	let { chosen, shown, onselectall, ondone, onchanged }: Props = $props();

	let busy = $state(false);
	let menu = $state<'status' | 'shelf' | null>(null);
	let shelves = $state<Shelf[] | null>(null);
	let creating = $state(false);
	let height = $state(0);

	const ids = $derived(chosen.map((i) => i.id));
	const none = $derived(chosen.length === 0 || busy);
	const allChosen = $derived(shown > 0 && chosen.length === shown);
	const allFavourite = $derived(chosen.length > 0 && chosen.every((i) => i.entry?.favourite));
	/** One title by name; more by how many. */
	const naming = $derived(chosen.length === 1 ? chosen[0].name : plural(chosen.length, 'title'));

	// Toasts stand above the bar, so Undo stays in reach and nothing is covered.
	$effect(() => {
		const root = document.documentElement.style;
		root.setProperty('--toast-lift', `${height}px`);
		return () => root.removeProperty('--toast-lift');
	});

	async function act(run: () => Promise<void>) {
		if (busy) return;
		busy = true;
		menu = null;
		try {
			await run();
			await onchanged();
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	/** Undo for an action that said what the titles held before it. */
	function undoable(message: string, before: EntryRecord[]) {
		toasts.undoable(message, async () => {
			try {
				await api.bulk({ action: 'restore', entries: before });
				await onchanged();
			} catch (e) {
				toasts.error(e);
			}
		});
	}

	function saidOfMarking(marked: number, skipped: number): string {
		const done = `Marked ${plural(marked, 'title')} watched`;
		return skipped ? `${done}. ${skipped} not out yet.` : done;
	}

	/** Asked once, and the answer goes for all of them. */
	function askWhen(): Promise<WhenAnswer | null> {
		const shows = chosen.some((i) => i.kind === 'tv');
		return whenWatched.ask(naming, shows ? 'Every aired episode' : 'Marked as watched', {
			film: !shows
		});
	}

	async function markWatched() {
		const answer = await askWhen();
		if (!answer) return;
		await act(async () => {
			const { marked, skipped } = await api.bulk({ action: 'watched', title_ids: ids, ...answer });
			toasts.push(saidOfMarking(marked, skipped));
		});
	}

	async function setStatus(status: Status) {
		// Completed means seen, as it does on a title's own page.
		if (status === 'completed') {
			const answer = await askWhen();
			if (!answer) return;
			await act(async () => {
				const { marked, skipped } = await api.bulk({
					action: 'status',
					status,
					title_ids: ids,
					...answer
				});
				toasts.push(saidOfMarking(marked, skipped));
			});
			return;
		}
		await act(async () => {
			const { before } = await api.bulk({ action: 'status', status, title_ids: ids });
			undoable(`Moved ${naming} to ${STATUS_LABEL[status]}`, before);
		});
	}

	function toggleFavourite() {
		const favourite = !allFavourite;
		void act(async () => {
			const { before } = await api.bulk({ action: 'favourite', favourite, title_ids: ids });
			undoable(
				favourite ? `Added ${naming} to favourites` : `Took ${naming} off favourites`,
				before
			);
		});
	}

	function remove() {
		const name = naming;
		void act(async () => {
			const { before } = await api.bulk({ action: 'remove', title_ids: ids });
			undoable(`Removed ${name}`, before);
		});
	}

	async function openShelves() {
		menu = menu === 'shelf' ? null : 'shelf';
		if (!menu) return;
		try {
			shelves = (await api.shelves()).shelves.filter((s) => s.kind === 'manual');
		} catch (e) {
			menu = null;
			toasts.error(e);
		}
	}

	function addTo(shelf: Pick<Shelf, 'id' | 'name'>) {
		const name = naming;
		void act(async () => {
			await api.bulk({ action: 'shelf', shelf_id: shelf.id, title_ids: ids });
			toasts.push(`Added ${name} to ${shelf.name}`);
		});
	}

	function onKey(event: KeyboardEvent) {
		if (event.key !== 'Escape' || whenWatched.pending || creating) return;
		if (menu) menu = null;
		else ondone();
	}
</script>

<svelte:window
	onkeydown={onKey}
	onclick={(e) => {
		if (menu && !(e.target as Element).closest('.has-menu')) menu = null;
	}}
/>

<div class="bar" role="toolbar" aria-label="Edit your library" bind:clientHeight={height}>
	<div class="top">
		<strong class="count" aria-live="polite">{chosen.length} selected</strong>
		<button class="link" disabled={!shown} onclick={onselectall}>
			{allChosen ? 'Select none' : 'Select all'}
		</button>
		<button class="btn primary done" onclick={ondone}>Done</button>
	</div>

	<div class="actions">
		<button class="act" disabled={none} onclick={markWatched}>
			<Icon name="check" size={19} /><span>Watched</span>
		</button>

		<div class="has-menu">
			<button
				class="act"
				disabled={none}
				aria-haspopup="menu"
				aria-expanded={menu === 'status'}
				onclick={() => (menu = menu === 'status' ? null : 'status')}
			>
				<Icon name="filter" size={19} /><span>Status</span>
			</button>
			{#if menu === 'status'}
				<div class="menu" role="menu" aria-label="Status">
					{#each STATUSES as s (s)}
						<button role="menuitem" data-status={s} onclick={() => setStatus(s)}>
							<i class="sdot" aria-hidden="true"></i>
							{STATUS_LABEL[s]}
						</button>
					{/each}
				</div>
			{/if}
		</div>

		<button class="act" disabled={none} aria-pressed={allFavourite} onclick={toggleFavourite}>
			<Icon name="heart" size={19} filled={allFavourite} /><span>Favourite</span>
		</button>

		<div class="has-menu">
			<button
				class="act"
				disabled={none}
				aria-haspopup="menu"
				aria-expanded={menu === 'shelf'}
				onclick={openShelves}
			>
				<Icon name="layers" size={19} /><span>Shelf</span>
			</button>
			{#if menu === 'shelf' && shelves}
				<div class="menu" role="menu" aria-label="Add to a shelf">
					{#each shelves as shelf (shelf.id)}
						<button role="menuitem" onclick={() => addTo(shelf)}>{shelf.name}</button>
					{/each}
					{#if shelves.length}<hr />{/if}
					<button
						role="menuitem"
						onclick={() => {
							menu = null;
							creating = true;
						}}
					>
						<Icon name="plus" size={15} /> New shelf
					</button>
				</div>
			{/if}
		</div>

		<button class="act danger" disabled={none} onclick={remove}>
			<Icon name="trash" size={19} /><span>Remove</span>
		</button>
	</div>
</div>

{#if creating}
	<NewShelf
		onclose={() => (creating = false)}
		oncreated={(shelf) => {
			creating = false;
			if (shelf.kind === 'manual') addTo(shelf);
			else toasts.push(`Made ${shelf.name}`);
		}}
	/>
{/if}

<style>
	/* In front of the tab bar on a phone, as an edit toolbar is. */
	.bar {
		position: fixed;
		z-index: 45;
		left: 50%;
		bottom: calc(16px + env(safe-area-inset-bottom));
		transform: translateX(-50%);
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: min(560px, calc(100vw - 32px));
		padding: 10px 12px;
		border-radius: var(--radius);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev);
		box-shadow: var(--shadow-3);
	}

	.top {
		display: flex;
		align-items: center;
		gap: 14px;
		min-height: 40px;
	}

	.count {
		font-size: 0.92rem;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.link {
		padding: 8px 4px;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		font-size: 0.86rem;
		font-weight: 600;
		white-space: nowrap;
		cursor: pointer;
	}

	.done {
		margin-left: auto;
		padding: 8px 18px;
	}

	.actions {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 4px;
	}

	.has-menu {
		position: relative;
		display: flex;
		min-width: 0;
	}

	.act {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 3px;
		width: 100%;
		min-width: 0;
		min-height: 52px;
		padding: 6px 2px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		font-size: 0.72rem;
		font-weight: 600;
		cursor: pointer;
	}

	.act:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.act:disabled {
		color: var(--text-faint);
		cursor: default;
	}

	.act.danger:not(:disabled) {
		color: var(--bad);
	}

	.menu {
		position: absolute;
		z-index: 1;
		left: 50%;
		bottom: calc(100% + 10px);
		transform: translateX(-50%);
		min-width: 190px;
		max-width: min(280px, 80vw);
		max-height: 46vh;
		overflow-y: auto;
		padding: 6px;
		border-radius: var(--radius);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		box-shadow: var(--shadow-3);
	}

	.menu button {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		min-height: 44px;
		padding: 8px 10px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		font-size: 0.86rem;
		text-align: left;
		cursor: pointer;
	}

	.menu button:hover {
		background: var(--bg-hover);
	}

	.menu hr {
		margin: 4px 0;
		border: none;
		border-top: 1px solid var(--line);
	}

	.sdot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--status);
	}

	@media (max-width: 760px) {
		.bar {
			left: 0;
			right: 0;
			bottom: 0;
			transform: none;
			width: auto;
			padding: 8px 12px calc(8px + env(safe-area-inset-bottom));
			border-radius: var(--radius-lg) var(--radius-lg) 0 0;
			border-width: 1px 0 0;
		}

		/* The outer two stay on screen. */
		.has-menu:nth-child(2) .menu {
			left: 0;
			transform: none;
		}

		.has-menu:nth-child(4) .menu {
			left: auto;
			right: 0;
			transform: none;
		}
	}
</style>
