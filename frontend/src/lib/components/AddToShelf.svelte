<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { anchorTo } from '#lib/onscreen';
	import { portal } from '#lib/portal';
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
	// Back closes it on a phone, as Esc does on a computer.
	backCloses(
		() => open,
		() => (open = false)
	);
	let creating = $state(false);
	let shelves = $state<{ id: number; name: string; on: boolean }[] | null>(null);
	let root: HTMLDivElement | undefined = $state();
	let button: HTMLButtonElement | undefined = $state();
	// The menu lives at the end of the page while open, so nothing the title
	// page draws around it can clip it, and on a phone it is a sheet.
	let panel: HTMLDivElement | undefined = $state();

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
		bind:this={button}
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label="Add to shelf"
		title="Add to shelf"
		onclick={toggleMenu}
	>
		<Icon name="layers" size={17} /> <span class="label">Add to shelf</span>
	</button>

	{#if open && shelves}
		<div class="layer" use:portal>
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="scrim" onclick={() => (open = false)}></div>
			<div class="menu" role="menu" aria-label="Shelves" bind:this={panel} use:anchorTo={button}>
				<div class="handle" aria-hidden="true"></div>
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

	/* Placed by `anchorTo` under its button, inside the window. */
	.menu {
		position: fixed;
		z-index: 70;
		left: 0;
		top: 0;
		min-width: 220px;
		max-width: min(320px, 86vw);
		max-height: 320px;
		overflow-y: auto;
		padding: 6px;
		border-radius: var(--radius);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		/* Grows out of what opened it. */
		animation: pop 180ms var(--spring) backwards;
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
		background: var(--bg-hover);
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

	.scrim,
	.handle {
		display: none;
	}

	/* A phone: a sheet along the bottom, as a card's menu is, over a dimmed
	   page. Anchored, it ran past the right edge and widened the page. */
	@media (max-width: 600px) {
		.scrim {
			display: block;
			position: fixed;
			inset: 0;
			z-index: 199;
			background: rgb(0 0 0 / 0.45);
			animation: scrim-in 160ms ease backwards;
		}

		.menu {
			z-index: 200;
			inset: auto 0 0 0;
			min-width: 0;
			max-width: none;
			max-height: 80dvh;
			padding: 0 10px calc(14px + env(safe-area-inset-bottom));
			border-radius: var(--radius-lg) var(--radius-lg) 0 0;
			border-width: 1px 0 0;
			animation: rise 220ms var(--ease) backwards;
		}

		.handle {
			display: block;
			width: 42px;
			height: 4px;
			border-radius: var(--radius-pill);
			background: var(--line-strong);
			margin: 10px auto 4px;
		}

		.item {
			min-height: 48px;
			padding: 10px 12px;
			font-size: 0.96rem;
		}
	}

	@keyframes rise {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}

	@keyframes scrim-in {
		from {
			opacity: 0;
		}
	}
</style>
