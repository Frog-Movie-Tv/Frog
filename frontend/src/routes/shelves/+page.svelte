<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import NewShelf from '#lib/components/NewShelf.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { Shelf } from '#lib/types';
	import { plural } from '#lib/format';

	let shelves = $state<Shelf[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let creating = $state(false);
	let arranging = $state(false);

	$effect(() => {
		if (!session.authenticated) return;
		void api
			.shelves()
			.then((r) => (shelves = r.shelves))
			.catch((e) => (error = e instanceof Error ? e.message : 'Could not load shelves'))
			.finally(() => (loading = false));
	});

	async function saveOrder() {
		try {
			await api.reorderShelves(shelves.map((s) => s.id));
		} catch (e) {
			toasts.error(e);
		}
	}

	async function created(shelf: Shelf) {
		creating = false;
		await goto(`/shelves/${shelf.id}${shelf.kind === 'smart' ? '?edit=1' : ''}`);
	}
</script>

<svelte:head>
	<title>Shelves · Frog</title>
</svelte:head>

<div class="page">
	<div class="head">
		<h1>Shelves</h1>
		<div class="controls">
			{#if shelves.length > 1}
				<button class="btn" class:primary={arranging} onclick={() => (arranging = !arranging)}>
					{#if arranging}<Icon name="check" size={16} /> Done{:else}<Icon name="grip" size={16} /> Arrange{/if}
				</button>
			{/if}
			<button class="btn primary" onclick={() => (creating = true)}>
				<Icon name="plus" size={16} /> New shelf
			</button>
		</div>
	</div>

	{#if loading}
		<Spinner label="Loading shelves" />
	{:else if error}
		<Empty title="Could not load shelves" hint={error} />
	{:else if shelves.length === 0}
		<Empty title="No shelves yet" hint="Group titles your way, by hand or by rules.">
			<button class="btn primary" onclick={() => (creating = true)}>New shelf</button>
		</Empty>
	{:else}
		<ol
			class="shelves"
			class:arranging
			use:sortable={{
				onmove: (from, to) => (shelves = moved(shelves, from, to)),
				onend: saveOrder
			}}
		>
			{#each shelves as shelf (shelf.id)}
				<li data-sort-item>
					<svelte:element
						this={arranging ? 'div' : 'a'}
						class="shelf card"
						href={arranging ? undefined : `/shelves/${shelf.id}`}
					>
						<span class="cover" aria-hidden="true">
							{#each [0, 1, 2, 3] as i (i)}
								{@const src = img(shelf.posters[i], 'w185')}
								<span class="tile"
									>{#if src}<img {src} alt="" loading="lazy" />{/if}</span
								>
							{/each}
						</span>
						<span class="meta">
							<strong>{shelf.name}</strong>
							<span class="faint">
								{#if shelf.kind === 'smart'}<Icon name="sparkle" size={13} /> Smart ·{/if}
								{plural(shelf.count, 'title')}
							</span>
						</span>
						{#if arranging}
							<button class="handle hit" data-sort-handle aria-label="Move {shelf.name}">
								<Icon name="grip" size={18} />
							</button>
						{/if}
					</svelte:element>
				</li>
			{/each}
		</ol>
	{/if}
</div>

{#if creating}
	<NewShelf oncreated={created} onclose={() => (creating = false)} />
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 14px;
		margin-bottom: 22px;
	}

	.controls {
		display: flex;
		gap: 10px;
	}

	.shelves {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 240px), 1fr));
		gap: 16px;
	}

	.shelf {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 12px;
		color: inherit;
		text-decoration: none;
		transition:
			border-color var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	a.shelf:hover {
		border-color: var(--line-strong);
	}

	.cover {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 4px;
		border-radius: var(--radius-sm);
		overflow: hidden;
	}

	.tile {
		aspect-ratio: 2 / 3;
		background: var(--bg-elev-2);
	}

	.tile img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
		padding: 0 2px 2px;
	}

	.meta strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta .faint {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 0.8rem;
	}

	.handle {
		position: absolute;
		right: 10px;
		bottom: 12px;
		padding: 8px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		cursor: grab;
		/* The handle owns the gesture; without this a touch drag scrolls. */
		touch-action: none;
	}

	.arranging :global([data-dragging] .shelf) {
		border-color: var(--accent);
		transform: scale(1.02);
		box-shadow: var(--shadow-3);
	}

	:global([data-sorting]) .handle {
		cursor: grabbing;
	}
</style>
