<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Grid from '#lib/components/Grid.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import ShelfRules from '#lib/components/ShelfRules.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { year, plural } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import { settings } from '#lib/settings.svelte';
	import type { LibraryItem, Shelf, ShelfRules as Rules } from '#lib/types';

	const id = $derived(Number(page.params.id));

	let shelf = $state<Shelf | null>(null);
	let items = $state<LibraryItem[]>([]);
	let loading = $state(true);
	let error = $state<Failure | null>(null);
	let editing = $state(false);
	let editingRules = $state(page.url.searchParams.get('edit') === '1');
	let renaming = $state(false);
	let draftName = $state('');

	async function load() {
		try {
			const r = await api.shelf(id);
			shelf = r.shelf;
			items = r.items;
			error = null;
		} catch (e) {
			error = failure(e, {
				title: 'No shelf here',
				hint: 'It may have been deleted, or it belongs to someone else.'
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (!session.authenticated || !id) return;
		void load();
	});

	const homeRow = $derived(`shelf:${id}`);
	const onHome = $derived(settings.current.homeRows.includes(homeRow));

	function toggleHome() {
		const adding = !onHome;
		const rows = settings.current.homeRows;
		settings.set('homeRows', adding ? [...rows, homeRow] : rows.filter((r) => r !== homeRow));
		toasts.push(adding ? 'Shown on Home' : 'Taken off Home');
	}

	async function saveOrder() {
		try {
			await api.reorderShelf(
				id,
				items.map((i) => i.id)
			);
		} catch (e) {
			toasts.error(e);
		}
	}

	async function remove(item: LibraryItem) {
		const before = items;
		items = items.filter((i) => i.id !== item.id);
		try {
			await api.removeFromShelf(id, item.id);
			if (shelf) shelf.count = items.length;
			toasts.undoable(`Removed ${item.name}`, async () => {
				await api.addToShelf(id, item.id);
				await api.reorderShelf(
					id,
					before.map((i) => i.id)
				);
				await load();
			});
		} catch (e) {
			items = before;
			toasts.error(e);
		}
	}

	function startRename() {
		draftName = shelf?.name ?? '';
		renaming = true;
	}

	async function rename(event: SubmitEvent) {
		event.preventDefault();
		if (!shelf || !draftName.trim()) return;
		try {
			const r = await api.updateShelf(id, { name: draftName.trim() });
			shelf = r.shelf;
			renaming = false;
		} catch (e) {
			toasts.error(e);
		}
	}

	async function saveRules(rules: Rules) {
		try {
			await api.updateShelf(id, { rules });
			editingRules = false;
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	async function destroy() {
		if (!shelf) return;
		const name = shelf.name;
		try {
			const gone = await api.deleteShelf(id);
			await goto('/shelves');
			toasts.undoable(`Deleted ${name}`, async () => {
				const { shelf: back } = await api.createShelf(gone);
				await goto(`/shelves/${back.id}`);
			});
		} catch (e) {
			toasts.error(e);
		}
	}
</script>

<svelte:head>
	<title
		>{shelf ? `${shelf.name} · Frog` : error ? `${error.heading} · Frog` : 'Shelf · Frog'}</title
	>
</svelte:head>

<div class="page">
	<a class="back faint" href="/shelves"><Icon name="chevron-left" size={16} /> Shelves</a>

	{#if loading}
		<Spinner label="Loading shelf" />
	{:else if error || !shelf}
		<Empty
			art={error && !error.missing ? 'rest' : 'search'}
			title={error?.title ?? 'No shelf here'}
			hint={error?.hint}
		>
			{#if error && !error.missing}
				<button class="btn primary" onclick={load}>Try again</button>
			{/if}
			<a class="btn" class:primary={!error || error.missing} href="/shelves">All shelves</a>
		</Empty>
	{:else}
		<div class="head">
			<div class="title">
				{#if renaming}
					<form class="rename" onsubmit={rename}>
						<!-- svelte-ignore a11y_autofocus -->
						<input
							class="input"
							bind:value={draftName}
							maxlength="60"
							aria-label="Shelf name"
							autofocus
						/>
						<button class="btn primary">Save</button>
						<button type="button" class="btn ghost" onclick={() => (renaming = false)}
							>Cancel</button
						>
					</form>
				{:else}
					<h1>{shelf.name}</h1>
				{/if}
				<span class="faint sub">
					{#if shelf.kind === 'smart'}<Icon name="sparkle" size={14} /> Smart shelf ·{/if}
					{plural(items.length, 'title')}
				</span>
			</div>
			<div class="controls">
				{#if shelf.kind === 'smart'}
					<button
						class="btn"
						class:primary={editingRules}
						onclick={() => (editingRules = !editingRules)}
					>
						<Icon name="filter" size={16} /> Rules
					</button>
				{:else if items.length}
					<button class="btn" class:primary={editing} onclick={() => (editing = !editing)}>
						{#if editing}<Icon name="check" size={16} /> Done{:else}<Icon name="grip" size={16} /> Edit{/if}
					</button>
				{/if}
				<button
					class="btn ghost icon"
					class:on-home={onHome}
					aria-pressed={onHome}
					aria-label="Show on Home"
					title={onHome ? 'On Home' : 'Show on Home'}
					onclick={toggleHome}
				>
					<Icon name="home" size={16} />
				</button>
				<button class="btn ghost icon" aria-label="Rename" title="Rename" onclick={startRename}>
					<Icon name="pencil" size={16} />
				</button>
				<button
					class="btn ghost icon"
					aria-label="Delete shelf"
					title="Delete shelf"
					onclick={destroy}
				>
					<Icon name="trash" size={16} />
				</button>
			</div>
		</div>

		{#if shelf.kind === 'smart' && editingRules}
			<ShelfRules rules={shelf.rules} onsave={saveRules} oncancel={() => (editingRules = false)} />
		{/if}

		{#if items.length === 0}
			{#if shelf.kind === 'smart'}
				<Empty title="Nothing matches yet" hint="Loosen the rules to let more in." art="search">
					{#if !editingRules}
						<button class="btn primary" onclick={() => (editingRules = true)}>Edit rules</button>
					{/if}
				</Empty>
			{:else}
				<Empty
					title="Nothing on this shelf yet"
					hint="Add titles from their page with Add to shelf."
				>
					<a class="btn primary" href="/search">Find something</a>
				</Empty>
			{/if}
		{:else if editing}
			<p class="faint hint">Drag to reorder.</p>
			<ol
				class="arrange"
				use:sortable={{ onmove: (from, to) => (items = moved(items, from, to)), onend: saveOrder }}
			>
				{#each items as item (item.id)}
					{@const src = img(item.poster_path, 'w342')}
					<li data-sort-item>
						<button class="handle" data-sort-handle aria-label="Move {item.name}">
							<span class="cover"
								>{#if src}<img {src} alt="" draggable="false" />{:else}<span class="faint"
										>{item.name}</span
									>{/if}</span
							>
							<span class="name">{item.name}</span>
						</button>
						<button class="remove" aria-label="Remove {item.name}" onclick={() => remove(item)}>
							<Icon name="x" size={14} />
						</button>
					</li>
				{/each}
			</ol>
		{:else}
			<Grid>
				{#each items as item (item.id)}
					<Poster
						morph
						kind={item.kind}
						ref={item}
						name={item.name}
						posterPath={item.poster_path}
						year={year(item.release_date)}
						status={item.entry?.status ?? null}
						rating={item.entry?.rating ?? null}
						watched={item.watched_episodes}
						total={item.total_episodes}
					/>
				{/each}
			</Grid>
		{/if}
	{/if}
</div>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.86rem;
		margin-bottom: 10px;
		text-decoration: none;
	}

	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 14px;
		margin-bottom: 22px;
	}

	.title {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.sub {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 0.86rem;
	}

	.rename {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	.rename .input {
		width: min(320px, 70vw);
	}

	.controls {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.icon {
		padding: 10px;
	}

	.on-home {
		color: var(--accent);
		border-color: var(--accent);
	}

	.hint {
		font-size: 0.82rem;
		margin-bottom: 12px;
	}

	.arrange {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--poster-min), 1fr));
		gap: var(--gap);
	}

	.arrange li {
		position: relative;
	}

	.handle {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		text-align: inherit;
		cursor: grab;
		touch-action: none;
		border-radius: var(--radius-sm);
	}

	.cover {
		display: grid;
		place-items: center;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		border: 1px solid var(--line);
		font-size: 0.8rem;
		text-align: center;
		padding: 0;
	}

	.cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
		user-select: none;
	}

	.name {
		display: block;
		margin-top: 6px;
		font-size: 0.82rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.arrange :global([data-dragging] .handle) {
		outline: 2px solid var(--accent);
		outline-offset: 3px;
		opacity: 0.85;
	}

	:global([data-sorting]) .handle {
		cursor: grabbing;
	}

	.remove {
		position: absolute;
		top: -8px;
		right: -8px;
		width: 30px;
		height: 30px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev);
		color: var(--text);
		cursor: pointer;
		box-shadow: var(--shadow-2, 0 2px 6px rgba(0, 0, 0, 0.3));
	}

	.remove:hover {
		background: var(--bad);
		color: #fff;
	}
</style>
