<script lang="ts">
	import Icon from './Icon.svelte';
	import NewShelf from './NewShelf.svelte';
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';
	import type { Shelf } from '#lib/types';

	interface Props {
		/** Frog's id for the title. */
		titleId: number;
		name: string;
	}

	let { titleId, name }: Props = $props();

	let open = $state(false);
	let creating = $state(false);
	let shelves = $state<{ id: number; name: string; on: boolean }[] | null>(null);
	let root: HTMLDivElement | undefined = $state();

	async function toggleMenu() {
		open = !open;
		if (open) {
			try {
				shelves = (await api.titleShelves(titleId)).shelves;
			} catch (e) {
				toasts.error(e);
				open = false;
			}
		}
	}

	async function toggle(shelf: { id: number; name: string; on: boolean }) {
		const on = !shelf.on;
		shelf.on = on;
		try {
			if (on) await api.addToShelf(shelf.id, titleId);
			else await api.removeFromShelf(shelf.id, titleId);
			toasts.push(on ? `Added to ${shelf.name}` : `Removed from ${shelf.name}`);
		} catch (e) {
			shelf.on = !on;
			toasts.error(e);
		}
	}

	function created(shelf: Shelf) {
		creating = false;
		open = false;
		toasts.push(`Added ${name} to ${shelf.name}`);
	}
</script>

<svelte:window
	onclick={(e) => {
		if (open && root && !root.contains(e.target as Node)) open = false;
	}}
	onkeydown={(e) => {
		if (open && e.key === 'Escape') open = false;
	}}
/>

<div class="add" bind:this={root}>
	<button
		class="btn"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label="Add to shelf"
		title="Add to shelf"
		onclick={toggleMenu}
	>
		<Icon name="layers" size={17} /> <span class="label">Add to shelf</span>
	</button>

	{#if open && shelves}
		<div class="menu" role="menu" aria-label="Shelves">
			{#each shelves as shelf (shelf.id)}
				<button
					class="item"
					role="menuitemcheckbox"
					aria-checked={shelf.on}
					onclick={() => toggle(shelf)}
				>
					<span class="tick"
						>{#if shelf.on}<Icon name="check" size={15} />{/if}</span
					>
					<span class="label">{shelf.name}</span>
				</button>
			{/each}
			{#if shelves.length}<hr />{/if}
			<button
				class="item"
				role="menuitem"
				onclick={() => {
					open = false;
					creating = true;
				}}
			>
				<span class="tick"><Icon name="plus" size={15} /></span>
				<span class="label">New shelf</span>
			</button>
		</div>
	{/if}
</div>

{#if creating}
	<NewShelf {titleId} oncreated={created} onclose={() => (creating = false)} />
{/if}

<style>
	.add {
		position: relative;
		display: inline-flex;
	}

	.menu {
		position: absolute;
		z-index: 40;
		top: calc(100% + 6px);
		left: 0;
		min-width: 220px;
		max-width: min(320px, 86vw);
		max-height: 320px;
		overflow-y: auto;
		padding: 6px;
		border-radius: var(--radius);
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
	}

	.item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 9px 10px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		text-align: left;
		font-size: 0.9rem;
		cursor: pointer;
	}

	.item:hover,
	.item:focus-visible {
		background: var(--bg-elev-2);
	}

	.tick {
		width: 16px;
		display: grid;
		place-items: center;
		color: var(--accent);
	}

	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	hr {
		border: 0;
		border-top: 1px solid var(--line);
		margin: 4px 2px;
	}
</style>
