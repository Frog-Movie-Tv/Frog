<script lang="ts">
	import { fly, fade } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import { whenWatched, type WhenMode } from '#lib/when.svelte';

	const today = new Date().toISOString().slice(0, 10);

	let mode = $state<WhenMode>('release');
	let date = $state(today);
	let dialog: HTMLDivElement | undefined = $state();

	const pending = $derived(whenWatched.pending);

	// Every time the dialog opens, start from the most likely answer for a
	// back catalogue: on the dates it aired.
	$effect(() => {
		if (pending) {
			mode = 'release';
			date = today;
			queueMicrotask(() => dialog?.querySelector<HTMLButtonElement>('.option.on')?.focus());
		}
	});

	const options = $derived(
		[
			{ id: 'today', label: 'Today', hint: 'Just now', icon: 'clock' },
			{ id: 'release', label: 'On release dates', hint: 'Each episode on the day it aired', icon: 'calendar' },
			{ id: 'date', label: 'On a date', hint: 'Pick one day for all of them', icon: 'calendar' },
			{ id: 'unknown', label: "I don't know", hint: 'Counted as watched, left out of the diary', icon: 'eye-off' }
		].filter((o) => pending?.allowToday !== false || o.id !== 'today') as {
			id: WhenMode;
			label: string;
			hint: string;
			icon: 'clock' | 'calendar' | 'eye-off';
		}[]
	);

	function confirm() {
		whenWatched.answer(mode === 'date' ? { when: 'date', date } : { when: mode });
	}

	function onKey(event: KeyboardEvent) {
		if (!pending) return;
		if (event.key === 'Escape') {
			event.preventDefault();
			whenWatched.answer(null);
		}
	}
</script>

<svelte:window onkeydown={onKey} />

{#if pending}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" transition:fade={{ duration: 120 }} onclick={() => whenWatched.answer(null)}></div>

	<div
		bind:this={dialog}
		class="dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="when-title"
		transition:fly={{ y: 16, duration: 180 }}
	>
		<h2 id="when-title">When did you watch this?</h2>
		<p class="muted">{pending.title} · {pending.detail}</p>

		<div class="options" role="radiogroup" aria-labelledby="when-title">
			{#each options as option (option.id)}
				<button
					class="option"
					class:on={mode === option.id}
					role="radio"
					aria-checked={mode === option.id}
					onclick={() => (mode = option.id)}
				>
					<Icon name={option.icon} size={18} />
					<span><strong>{option.label}</strong><small>{option.hint}</small></span>
				</button>
			{/each}
		</div>

		{#if mode === 'date'}
			<label class="date">
				<span>Date</span>
				<input class="input" type="date" max={today} bind:value={date} />
			</label>
		{/if}

		<div class="actions">
			<button class="btn ghost" onclick={() => whenWatched.answer(null)}>Cancel</button>
			<button class="btn primary" disabled={mode === 'date' && !date} onclick={confirm}>
				Mark watched
			</button>
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 95;
		background: rgba(0, 0, 0, 0.55);
	}

	.dialog {
		position: fixed;
		z-index: 96;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		width: min(440px, calc(100vw - 28px));
		max-height: calc(100dvh - 40px);
		overflow-y: auto;
		padding: 22px;
		border-radius: var(--radius-lg);
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
	}

	h2 {
		font-size: 1.2rem;
	}

	.muted {
		margin-top: 6px;
		font-size: 0.86rem;
	}

	.options {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 18px 0 14px;
	}

	.option {
		display: flex;
		align-items: center;
		gap: 13px;
		padding: 12px 14px;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		text-align: left;
		cursor: pointer;
		transition: border-color var(--fast) var(--ease);
	}

	.option:hover {
		border-color: var(--line-strong);
	}

	.option.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
		color: var(--text);
	}

	.option span {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.option strong {
		font-size: 0.92rem;
		color: var(--text);
	}

	.option small {
		font-size: 0.76rem;
		color: var(--text-faint);
	}

	.date {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 14px;
		font-size: 0.8rem;
		color: var(--text-dim);
	}

	.date input {
		color-scheme: inherit;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
	}
</style>
