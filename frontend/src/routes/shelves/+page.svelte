<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import NewShelf from '#lib/components/NewShelf.svelte';
	import ShelfCard from '#lib/components/ShelfCard.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { session } from '#lib/session.svelte';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { Shelf } from '#lib/types';

	let shelves = $state<Shelf[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// Home's "New shelf" tile lands here with ?new=1.
	let creating = $state(page.url.searchParams.has('new'));
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
	<div class="page-head">
		<div>
			<h1>Shelves</h1>
			{#if shelves.length}
				<p class="sub">Your library, grouped your way.</p>
			{/if}
		</div>
		<div class="actions">
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
					<ShelfCard {shelf} link={!arranging}>
						{#if arranging}
							<button class="handle hit" data-sort-handle aria-label="Move {shelf.name}">
								<Icon name="grip" size={18} />
							</button>
						{/if}
					</ShelfCard>
				</li>
			{/each}
		</ol>
	{/if}
</div>

{#if creating}
	<NewShelf oncreated={created} onclose={() => (creating = false)} />
{/if}

<style>
	.shelves {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 220px), 1fr));
		gap: 28px 20px;
	}

	.handle {
		position: absolute;
		right: 10px;
		top: 10px;
		z-index: 4;
		padding: 8px;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		cursor: grab;
		/* The handle owns the gesture; without this a touch drag scrolls. */
		touch-action: none;
	}

	.arranging li {
		transition: transform var(--fast) var(--ease);
	}

	.arranging :global([data-dragging]) {
		transform: scale(1.03);
	}

	.arranging :global([data-dragging] .cover) {
		box-shadow:
			0 0 0 2px var(--accent),
			var(--shadow-3);
	}

	:global([data-sorting]) .handle {
		cursor: grabbing;
	}

	@media (max-width: 520px) {
		.shelves {
			grid-template-columns: 1fr 1fr;
			gap: 22px 14px;
		}
	}
</style>
