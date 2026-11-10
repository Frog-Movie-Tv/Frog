<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { backCloses } from '#lib/layers.svelte';
	import { menu, type MenuEntry, type MenuItem } from '#lib/menu.svelte';
	import { SHEET_QUERY } from '#lib/onscreen';
	import Icon from './Icon.svelte';

	/*
	 * The one context menu, drawn where `menu.open` says. On a computer it
	 * opens at the pointer and is kept on screen; on a phone, or after a hold
	 * on any touch screen, it is a sheet along the bottom. A choice that has
	 * choices of its own opens as a second level in the same place, with a
	 * way back, rather than a second menu beside the first.
	 */

	/** One level: what it is called, and its entries, or null while they load. */
	interface Level {
		title: string | undefined;
		entries: MenuEntry[] | null;
	}

	let levels = $state<Level[]>([]);
	let busy = $state(false);
	let sheet = $state(false);
	/** Where the page stood when the menu opened: a scroll since is a change of mind. */
	let scrolledFrom = 0;
	let panel: HTMLDivElement | undefined = $state();

	const level = $derived(levels[levels.length - 1]);

	backCloses(
		() => !!menu.open,
		() => menu.close()
	);

	$effect(() => {
		const now = menu.open;
		if (!now) {
			levels = [];
			busy = false;
			return;
		}
		sheet = now.touch || matchMedia(SHEET_QUERY).matches;
		scrolledFrom = window.scrollY;
		levels = [{ title: now.source.title, entries: now.source.items() }];
	});

	// Placed once drawn: moved left or up as far as it takes to stay on
	// screen, and again whenever a level changes its size.
	$effect(() => {
		const now = menu.open;
		if (!now || !panel || sheet) return;
		void level;
		const pad = 8;
		const r = panel.getBoundingClientRect();
		let x = now.x;
		let y = now.y;
		if (x + r.width > innerWidth - pad) x = Math.max(pad, innerWidth - pad - r.width);
		if (y + r.height > innerHeight - pad) y = Math.max(pad, innerHeight - pad - r.height);
		panel.style.left = `${Math.round(x)}px`;
		panel.style.top = `${Math.round(y)}px`;
	});

	// The keyboard works from the first moment, without taking the focus ring
	// to an item nobody chose.
	$effect(() => {
		if (menu.open && panel) panel.focus({ preventScroll: true });
	});

	afterNavigate(() => menu.close());

	async function pick(item: MenuItem) {
		if (busy || item.disabled) return;
		if (item.children) {
			const depth = levels.length;
			levels = [...levels, { title: item.label, entries: null }];
			let entries: MenuEntry[] = [];
			try {
				entries = await item.children();
			} catch {
				// Nothing to list: the level says so.
			}
			// Still the level that asked, not one closed or left since.
			if (levels.length === depth + 1 && levels[depth].title === item.label) {
				levels[depth].entries = entries;
			}
			return;
		}
		busy = true;
		try {
			await item.run?.();
		} finally {
			menu.close();
		}
	}

	function back() {
		if (levels.length > 1) levels = levels.slice(0, -1);
	}

	function items(): HTMLElement[] {
		return Array.from(panel?.querySelectorAll<HTMLElement>('[role^="menuitem"]:enabled') ?? []);
	}

	function onkeydown(e: KeyboardEvent) {
		const list = items();
		const at = list.indexOf(document.activeElement as HTMLElement);
		const go = (i: number) => list[(i + list.length) % list.length]?.focus();
		switch (e.key) {
			case 'Escape':
				menu.close();
				break;
			case 'ArrowDown':
				go(at + 1);
				break;
			case 'ArrowUp':
				go(at < 0 ? list.length - 1 : at - 1);
				break;
			case 'Home':
				go(0);
				break;
			case 'End':
				go(list.length - 1);
				break;
			case 'ArrowLeft':
				back();
				break;
			case 'ArrowRight': {
				const entry = level?.entries?.find(
					(x) => x !== 'rule' && x.label === list[at]?.dataset.label
				);
				if (entry && entry !== 'rule' && entry.children) void pick(entry);
				break;
			}
			default:
				return;
		}
		e.preventDefault();
	}
</script>

<!-- A scroll under an open menu is a change of mind. -->
<svelte:window
	onscroll={() => {
		// The event for a scroll that brought the card into view can land
		// after the click that opened the menu; only a real move closes it.
		if (menu.open && !sheet && Math.abs(window.scrollY - scrolledFrom) > 4) menu.close();
	}}
	onresize={() => menu.close()}
/>

{#if menu.open && level}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="catch"
		class:scrim={sheet}
		onpointerdown={() => menu.close()}
		oncontextmenu={(e) => {
			e.preventDefault();
			menu.close();
		}}
	></div>
	<div
		class="menu"
		class:sheet
		class:busy
		role="menu"
		aria-label={levels[0].title ?? 'Actions'}
		tabindex="-1"
		bind:this={panel}
		{onkeydown}
	>
		{#if sheet}<div class="handle" aria-hidden="true"></div>{/if}
		{#if levels.length > 1}
			<button class="item back" role="menuitem" onclick={back}>
				<span class="ic"><Icon name="chevron-left" size={16} /></span>
				<span class="label">{level.title}</span>
			</button>
			<hr />
		{:else if level.title}
			<div class="title">{level.title}</div>
		{/if}

		{#if level.entries === null}
			<div class="loading faint">Loading</div>
		{:else if level.entries.length === 0}
			<div class="loading faint">Nothing here</div>
		{:else}
			{#each level.entries as entry, i (i)}
				{#if entry === 'rule'}
					<hr />
				{:else}
					<button
						class="item"
						class:danger={entry.danger}
						role={entry.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
						aria-checked={entry.checked}
						disabled={entry.disabled || busy}
						data-label={entry.label}
						onclick={() => pick(entry)}
					>
						<span class="ic">
							{#if entry.checked}<Icon name="check" size={16} />{:else if entry.icon}<Icon
									name={entry.icon}
									size={16}
								/>{/if}
						</span>
						<span class="label">{entry.label}</span>
						{#if entry.children}<span class="more"><Icon name="chevron-right" size={15} /></span
							>{/if}
					</button>
				{/if}
			{/each}
		{/if}
	</div>
{/if}

<style>
	.catch {
		position: fixed;
		inset: 0;
		z-index: 80;
	}

	.scrim {
		background: rgba(0, 0, 0, 0.5);
		animation: fade 160ms var(--ease) backwards;
	}

	@keyframes fade {
		from {
			opacity: 0;
		}
	}

	.menu {
		position: fixed;
		z-index: 81;
		min-width: 230px;
		max-width: min(320px, calc(100vw - 16px));
		max-height: calc(100dvh - 16px);
		overflow-y: auto;
		padding: 6px;
		border-radius: var(--radius);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		outline: none;
		/* Grows out of where it was asked for. */
		transform-origin: top left;
		animation: pop 160ms var(--spring) backwards;
	}

	.title {
		padding: 8px 12px 6px;
		font-size: 0.78rem;
		font-weight: 600;
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		min-height: 38px;
		padding: 8px 10px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		font-size: 0.9rem;
		text-align: left;
		cursor: pointer;
	}

	.item:hover:not(:disabled),
	.item:focus-visible {
		background: var(--bg-hover);
		outline: none;
	}

	.item:disabled {
		cursor: default;
		opacity: 0.5;
	}

	.busy .item {
		cursor: progress;
	}

	.item.danger {
		color: var(--bad);
	}

	.ic {
		flex: none;
		width: 18px;
		display: grid;
		place-items: center;
		color: var(--text-dim);
	}

	.item[aria-checked='true'] .ic,
	.danger .ic {
		color: inherit;
	}

	.label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.more {
		flex: none;
		color: var(--text-faint);
	}

	.back .label {
		font-weight: 600;
	}

	.loading {
		padding: 12px;
		font-size: 0.86rem;
	}

	hr {
		border: 0;
		border-top: 1px solid var(--line);
		margin: 4px 2px;
	}

	/* A sheet along the bottom, where a thumb reaches it. */
	.menu.sheet {
		inset: auto 0 0 0;
		min-width: 0;
		max-width: none;
		max-height: 80dvh;
		padding: 0 10px calc(14px + env(safe-area-inset-bottom));
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		border-width: 1px 0 0;
		transform-origin: bottom center;
		animation: rise 220ms var(--ease) backwards;
	}

	@keyframes rise {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}

	.handle {
		width: 42px;
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		margin: 10px auto 4px;
	}

	.sheet .title {
		padding: 10px 12px 8px;
		font-size: 0.86rem;
	}

	.sheet .item {
		min-height: 48px;
		padding: 10px 12px;
		font-size: 0.96rem;
	}
</style>
