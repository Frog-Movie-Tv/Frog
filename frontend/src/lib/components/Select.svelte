<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { anchorTo, onScreen, SHEET_QUERY } from '#lib/onscreen';
	import Icon from './Icon.svelte';

	interface Option {
		value: string;
		label: string;
		hint?: string;
	}

	interface Props {
		value: string;
		options: Option[];
		onchange: (value: string) => void;
		/** Announced to screen readers and used as the tooltip. */
		label: string;
		compact?: boolean;
		full?: boolean;
	}

	let { value, options, onchange, label, compact = false, full = false }: Props = $props();

	let open = $state(false);
	/** Inside a dialog or sheet: a dropdown there too, never a sheet over a sheet. */
	let nested = $state(false);
	// Back closes it on a phone, as Esc does on a computer.
	backCloses(
		() => open,
		() => (open = false)
	);
	let root: HTMLDivElement | undefined = $state();
	let trigger: HTMLButtonElement | undefined = $state();
	let menu: HTMLUListElement | undefined = $state();
	let cursor = $state(0);

	const current = $derived(options.find((o) => o.value === value) ?? options[0]);

	function toggle() {
		nested = !!root?.parentElement?.closest('[role="dialog"]');
		open = !open;
		if (open)
			cursor = Math.max(
				0,
				options.findIndex((o) => o.value === value)
			);
	}

	function pick(option: Option) {
		open = false;
		if (option.value !== value) onchange(option.value);
	}

	/**
	 * The menu moves to the end of the page while open, so nothing it sits in
	 * can cut it off: a settings pane rounds its corners by hiding what runs
	 * past them, and a menu left inside one lost its last choices. On a
	 * computer it is then placed under its button and follows it; on a phone
	 * it is a sheet over everything, placed by its own CSS. Inside a dialog it
	 * stays where it is.
	 */
	function lift(node: HTMLElement, inPlace: boolean) {
		if (inPlace) return;
		document.body.appendChild(node);
		return { destroy: () => node.remove() };
	}

	function place(node: HTMLElement, inPlace: boolean) {
		return inPlace ? onScreen(node, false) : anchorTo(node, trigger);
	}

	/** On a phone, the sheet: its own CSS places it. */
	const isSheet = () => window.matchMedia(SHEET_QUERY).matches;

	function onKey(event: KeyboardEvent) {
		if (!open) {
			if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
				event.preventDefault();
				toggle();
			}
			return;
		}

		if (event.key === 'Escape') {
			event.preventDefault();
			open = false;
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			cursor = (cursor + 1) % options.length;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			cursor = (cursor - 1 + options.length) % options.length;
		} else if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			pick(options[cursor]);
		}
	}
</script>

<svelte:window
	onclick={(e) => {
		const target = e.target as Node;
		// The menu lives at the end of the page while open, outside `root`.
		if (open && root && !root.contains(target) && !menu?.contains(target)) open = false;
	}}
/>

<div class="select" class:compact class:full bind:this={root}>
	<button
		type="button"
		class="trigger"
		bind:this={trigger}
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-label={label}
		title={label}
		onclick={toggle}
		onkeydown={onKey}
	>
		<span class="current">{current?.label ?? ''}</span>
		<Icon name="chevron-down" size={15} />
	</button>

	{#if open}
		<div class="layer" use:lift={nested}>
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="scrim" class:nested onclick={() => (open = false)}></div>
			<ul
				class="menu"
				class:nested
				class:lifted={!nested && !isSheet()}
				bind:this={menu}
				use:place={nested}
				role="listbox"
				aria-label={label}
				tabindex="-1"
				onkeydown={onKey}
			>
				{#each options as option, i (option.value)}
					<li>
						<button
							type="button"
							class="option"
							class:on={option.value === value}
							class:cursor={i === cursor}
							role="option"
							aria-selected={option.value === value}
							onmouseenter={() => (cursor = i)}
							onclick={() => pick(option)}
						>
							<span class="tick">
								{#if option.value === value}<Icon name="check" size={14} />{/if}
							</span>
							<span class="text">
								{option.label}
								{#if option.hint}<small>{option.hint}</small>{/if}
							</span>
						</button>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</div>

<style>
	/* Only a wrapper to move: the menu inside places itself. */
	.layer {
		display: contents;
	}

	/* A styled menu rather than a native select: the native control cannot be
	   themed on any platform, so one dropdown looked like Windows and the rest
	   of the app looked like Frog. */
	.select {
		position: relative;
		display: inline-flex;
	}

	.select.full {
		display: flex;
		width: 100%;
	}

	.trigger {
		display: inline-flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		width: 100%;
		padding: 10px 13px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		color: var(--text);
		font-size: 0.88rem;
		font-weight: 550;
		cursor: pointer;
		white-space: nowrap;
		transition: border-color var(--fast) var(--ease);
	}

	.trigger:hover {
		border-color: var(--accent);
	}

	.compact .trigger {
		padding: 6px 10px;
		font-size: 0.8rem;
	}

	@media (max-width: 760px) {
		.trigger {
			min-height: 44px;
		}

		.option {
			min-height: 44px;
			align-items: center;
		}
	}

	.current {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.menu {
		position: absolute;
		z-index: 70;
		top: calc(100% + 6px);
		right: 0;
		min-width: 100%;
		max-height: 60vh;
		overflow-y: auto;
		list-style: none;
		margin: 0;
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

	/* Moved to the end of the page (see `lift`), pinned to its button by `anchorTo`. */
	.menu.lifted {
		position: fixed;
		z-index: 200;
		top: 0;
		left: 0;
		right: auto;
		min-width: 0;
	}

	.option {
		display: flex;
		align-items: flex-start;
		gap: 9px;
		width: 100%;
		padding: 8px 10px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text-dim);
		font-size: 0.86rem;
		text-align: left;
		cursor: pointer;
		white-space: nowrap;
	}

	.option.cursor {
		background: var(--bg-hover);
		color: var(--text);
	}

	.option.on {
		color: var(--text);
		font-weight: 600;
	}

	.tick {
		width: 14px;
		display: grid;
		place-items: center;
		color: var(--accent);
		padding-top: 2px;
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	small {
		font-size: 0.74rem;
		color: var(--text-faint);
		font-weight: 400;
	}

	/* A phone: the menu rises from the bottom, always whole and in reach,
	   over a dimmed page. */
	.scrim {
		display: none;
	}

	@media (max-width: 600px) {
		.scrim:not(.nested) {
			display: block;
			position: fixed;
			inset: 0;
			z-index: 199;
			background: rgb(0 0 0 / 0.45);
			animation: scrim-in 160ms ease backwards;
		}

		.menu:not(.nested) {
			position: fixed;
			z-index: 200;
			top: auto;
			left: 10px;
			right: 10px;
			bottom: calc(10px + env(safe-area-inset-bottom));
			min-width: 0;
			max-height: 70dvh;
			overflow-y: auto;
			border-radius: 18px;
			animation: sheet-up 220ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		}
	}

	@keyframes sheet-up {
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
