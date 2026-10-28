<script lang="ts">
	import { fly, fade } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import { whenWatched, type WhenMode } from '#lib/when.svelte';

	const today = new Date().toISOString().slice(0, 10);

	let mode = $state<WhenMode>('release');
	let date = $state(today);
	let picked = $state<number[]>([]);
	let dialog: HTMLDivElement | undefined = $state();

	const pending = $derived(whenWatched.pending);

	// Every time the dialog opens, start from the most likely answer for a
	// back catalogue: on the dates it aired.
	$effect(() => {
		if (pending) {
			mode = 'release';
			date = today;
			picked = [];
			queueMicrotask(() => dialog?.querySelector<HTMLButtonElement>('.option.on')?.focus());
		}
	});

	const options = $derived(
		[
			{ id: 'today', label: 'Today', hint: 'Just now', icon: 'clock' },
			{
				id: 'release',
				label: 'On release dates',
				hint: 'Each episode on the day it aired',
				icon: 'calendar'
			},
			{ id: 'date', label: 'On a date', hint: 'Pick one day for all of them', icon: 'calendar' },
			{
				id: 'unknown',
				label: "I don't know",
				hint: 'Counted as watched, left out of the diary',
				icon: 'eye-off'
			}
		].filter((o) => pending?.allowToday !== false || o.id !== 'today') as {
			id: WhenMode;
			label: string;
			hint: string;
			icon: 'clock' | 'calendar' | 'eye-off';
		}[]
	);

	function confirm() {
		const extras = picked.length ? { extras: picked } : {};
		whenWatched.answer(
			mode === 'date' ? { when: 'date', date, ...extras } : { when: mode, ...extras }
		);
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
	<div
		class="scrim"
		transition:fade={{ duration: 120 }}
		onclick={() => whenWatched.answer(null)}
	></div>

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

		{#if pending.extras.length}
			<fieldset class="extras">
				<legend>{pending.extrasLabel}</legend>
				<ul>
					{#each pending.extras as extra (extra.key)}
						<li>
							<label class="extra">
								<input type="checkbox" bind:group={picked} value={extra.key} />
								<span>
									<strong>{extra.label}</strong>
									{#if extra.hint}<small>{extra.hint}</small>{/if}
								</span>
							</label>
						</li>
					{/each}
				</ul>
			</fieldset>
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

	.extras {
		margin: 4px 0 14px;
		padding: 0;
		border: none;
	}

	.extras legend {
		padding: 0;
		margin-bottom: 8px;
		font-size: 0.84rem;
		font-weight: 650;
		color: var(--text-dim);
	}

	.extras ul {
		list-style: none;
		margin: 0;
		padding: 0;
		max-height: 220px;
		overflow-y: auto;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
	}

	.extras li + li {
		border-top: 1px solid var(--line);
	}

	.extra {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 9px 12px;
		cursor: pointer;
	}

	.extra input {
		width: 18px;
		height: 18px;
		flex: none;
		accent-color: var(--accent);
	}

	.extra span {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.extra strong {
		font-size: 0.86rem;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.extra small {
		font-size: 0.74rem;
		color: var(--text-faint);
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
