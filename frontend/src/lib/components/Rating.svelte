<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { onScreen, SHEET_QUERY } from '#lib/onscreen';
	import { formatScore, fromDisplay, scale, toDisplay } from '#lib/format';
	import { settings } from '#lib/settings.svelte';
	import { sliding } from '#lib/score-entry';
	import Icon from './Icon.svelte';
	import Score from './Score.svelte';
	import ScoreSlider from './ScoreSlider.svelte';
	import Stars from './Stars.svelte';

	interface Props {
		/** 0..100, or null for unrated. */
		value: number | null;
		onchange: (value: number | null) => void;
		/** Smaller type and tighter padding, for episode rows. */
		compact?: boolean;
		disabled?: boolean;
	}

	let { value, onchange, compact = false, disabled = false }: Props = $props();

	let open = $state(false);
	/** Inside a dialog or sheet: a dropdown there too, never a sheet over a sheet. */
	let nested = $state(false);
	// Back closes it on a phone, as Esc does on a computer.
	backCloses(
		() => open,
		() => (open = false)
	);
	let root: HTMLElement | undefined = $state();
	let pop: HTMLElement | undefined = $state();

	const format = $derived(settings.current.scoreFormat);
	const bounds = $derived(scale(format));
	const shown = $derived(toDisplay(value, format));
	const label = $derived(value === null ? 'Rate' : formatScore(value, format));

	/**
	 * The choices offered in the popover, in display units: ten at most, one
	 * comfortable tap each. A hundred points, or ten with a decimal, are slid
	 * to instead.
	 */
	const steps = $derived.by(() => {
		// The fine-grained formats slide instead (`ScoreSlider`).
		let step = bounds.step;
		if (format === 'star5') step = settings.current.halfStars ? 0.5 : 1;

		const out: number[] = [];
		for (let v = step; v <= bounds.max + 1e-9; v += step) out.push(Math.round(v * 10) / 10);
		return out;
	});

	function choose(display: number) {
		const same = shown !== null && Math.abs(shown - display) < 1e-6;
		onchange(same ? null : fromDisplay(display, format));
		open = false;
	}

	function onWindowClick(event: MouseEvent) {
		const target = event.target as Node;
		if (open && root && !root.contains(target) && !pop?.contains(target)) open = false;
	}

	/**
	 * The menu opens at the top of the page, not inside the button's own box:
	 * a title's hero and an episode row are layers of their own, and a menu
	 * left inside one ended under the tab bar with its last scores cut off.
	 * On a computer it sits under the button, or over it when there is no
	 * room below; on a phone it is a sheet above the tab bar (see the CSS).
	 * Inside a dialog it stays where it is, a dropdown, kept on screen.
	 */
	function lift(node: HTMLElement, inPlace: boolean) {
		const menu = node.querySelector<HTMLElement>('.pop');
		if (inPlace) {
			if (!menu) return;
			const kept = onScreen(menu, false);
			return { destroy: () => kept.destroy() };
		}
		document.body.appendChild(node);
		const place = () => {
			if (!menu || !root || window.matchMedia(SHEET_QUERY).matches) return;
			const pad = 8;
			const at = root.getBoundingClientRect();
			const tall = menu.offsetHeight;
			const below = window.innerHeight - at.bottom - pad;
			menu.style.top = '';
			menu.style.bottom = '';
			if (below >= tall + 6 || below >= at.top) {
				menu.style.top = `${Math.round(at.bottom + 6)}px`;
			} else {
				menu.style.bottom = `${Math.round(window.innerHeight - at.top + 6)}px`;
			}
			const width = menu.offsetWidth;
			const left = Math.min(Math.max(pad, at.right - width), window.innerWidth - width - pad);
			menu.style.left = `${Math.round(left)}px`;
		};
		place();
		window.addEventListener('resize', place);
		window.addEventListener('scroll', place, { passive: true, capture: true });
		return {
			destroy() {
				window.removeEventListener('resize', place);
				window.removeEventListener('scroll', place, { capture: true });
				node.remove();
			}
		};
	}

	function onKey(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			open = false;
			event.stopPropagation();
		}
	}
</script>

<svelte:window onclick={onWindowClick} onkeydown={onKey} />

<div class="rating" class:compact bind:this={root}>
	<button
		class="chip hit"
		class:rated={value !== null}
		{disabled}
		aria-haspopup={sliding(format) ? 'dialog' : 'menu'}
		aria-expanded={open}
		aria-label={value === null ? 'Rate this' : `Rated ${label}`}
		title={value === null ? 'Rate this' : `Rated ${label}`}
		onclick={() => {
			nested = !!root?.parentElement?.closest('[role="dialog"]');
			open = !open;
		}}
	>
		{#if format === 'star5' && value !== null}
			<span class="meter"><Stars value={shown ?? 0} outOf={5} size={14} /></span>
			<span class="sr">{label}</span>
		{:else}
			{#if !compact}<Icon name="star" size={15} filled={value !== null} />{/if}
			<span class="value">{label}</span>
		{/if}
	</button>

	{#if open}
		<div class="hold">
			<div class="layer" use:lift={nested}>
				<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
				<div class="scrim" class:nested onclick={() => (open = false)}></div>
				<div
					class="pop"
					class:nested
					class:lifted={!nested}
					bind:this={pop}
					role={sliding(format) ? 'dialog' : 'menu'}
					aria-label="Choose a rating"
				>
					<div class="head">
						<strong>Your rating</strong>
						{#if value !== null}
							<button
								class="clear"
								onclick={() => {
									onchange(null);
									open = false;
								}}>Clear</button
							>
						{/if}
					</div>
					{#if sliding(format)}
						<!-- A hundred points, or ten with a decimal: slid to, not picked. -->
						<ScoreSlider
							{format}
							{value}
							action="Save"
							autofocus
							onsubmit={(v) => {
								onchange(v);
								open = false;
							}}
						/>
					{:else}
						<div class="grid" data-format={format}>
							{#each steps as step (step)}
								<button
									class="opt"
									class:on={shown !== null && Math.abs(shown - step) < 1e-6}
									role="menuitemradio"
									aria-checked={shown !== null && Math.abs(shown - step) < 1e-6}
									onclick={() => choose(step)}
								>
									<Score value={fromDisplay(step, format)} />
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.rating {
		position: relative;
		display: inline-flex;
	}

	/* One small control that says what it is when empty, instead of a dash. */
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		min-width: 62px;
		justify-content: center;
		border-radius: var(--radius-pill);
		border: 1px dashed var(--line-strong);
		background: transparent;
		color: var(--text-dim);
		font-size: 0.84rem;
		font-weight: 650;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.chip:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--text);
	}

	.chip.rated {
		border-style: solid;
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
		background: var(--accent-soft);
		color: var(--text);
	}

	.chip:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.compact .chip {
		padding: 4px 10px;
		min-width: 50px;
		font-size: 0.76rem;
	}

	.value {
		font-variant-numeric: tabular-nums;
	}

	.meter {
		display: inline-flex;
		color: var(--accent);
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}

	.pop {
		position: absolute;
		z-index: 60;
		top: calc(100% + 6px);
		right: 0;
		padding: 10px;
		border-radius: var(--radius);
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
		min-width: max-content;
	}

	/* Held at the top of the page (see `lift`): placed against the window. */
	.pop.lifted {
		position: fixed;
		z-index: 200;
		top: auto;
		right: auto;
		max-height: calc(100dvh - 16px);
		overflow-y: auto;
	}

	/* In place, inside a dialog, the wrappers take no room of their own. */
	.hold,
	.layer {
		display: contents;
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		min-height: 30px;
		margin: -2px 2px 8px;
		font-size: 0.82rem;
	}

	.grid {
		display: grid;
		gap: 4px;
		grid-template-columns: repeat(5, minmax(34px, 1fr));
	}

	.grid[data-format='pt100'],
	.grid[data-format='pt10dec'] {
		grid-template-columns: repeat(5, minmax(42px, 1fr));
	}

	.grid[data-format='smiley3'] {
		grid-template-columns: repeat(3, 1fr);
	}

	.opt {
		padding: 7px 4px;
		border-radius: var(--radius-sm);
		border: 1px solid transparent;
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
		white-space: nowrap;
	}

	.opt:hover {
		background: var(--bg-hover);
		color: var(--text);
	}

	.opt.on {
		background: var(--accent);
		border-color: transparent;
		color: var(--accent-ink);
	}

	.clear {
		padding: 5px 12px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: transparent;
		color: var(--bad);
		font-size: 0.78rem;
		font-weight: 600;
		cursor: pointer;
	}

	.clear:hover {
		border-color: color-mix(in srgb, var(--bad) 50%, transparent);
		background: color-mix(in srgb, var(--bad) 10%, transparent);
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

		.pop:not(.nested) {
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
