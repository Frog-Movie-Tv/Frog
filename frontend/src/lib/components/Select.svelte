<script lang="ts">
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
	let root: HTMLDivElement | undefined = $state();
	let cursor = $state(0);

	const current = $derived(options.find((o) => o.value === value) ?? options[0]);

	function toggle() {
		open = !open;
		if (open) cursor = Math.max(0, options.findIndex((o) => o.value === value));
	}

	function pick(option: Option) {
		open = false;
		if (option.value !== value) onchange(option.value);
	}

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
		if (open && root && !root.contains(e.target as Node)) open = false;
	}}
/>

<div class="select" class:compact class:full bind:this={root}>
	<button
		type="button"
		class="trigger"
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
		<ul class="menu" role="listbox" aria-label={label} tabindex="-1" onkeydown={onKey}>
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
	{/if}
</div>

<style>
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
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
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
</style>
