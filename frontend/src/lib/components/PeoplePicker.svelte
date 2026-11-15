<script lang="ts">
	import { fade, fly } from 'svelte/transition';
	import { api } from '#lib/api';
	import { backCloses } from '#lib/layers.svelte';
	import { peoplePicker } from '#lib/people.svelte';
	import type { Choosable } from '#lib/types';
	import PeopleList from './PeopleList.svelte';

	/*
	 * The one people picker: a dialog on a computer, a sheet on a phone. Who
	 * is offered comes from the server (`/api/people/choosable`), which leaves
	 * out anyone whose profile is off or who does not want this.
	 */

	const NOTE_MAX = 280;

	let people = $state<Choosable[] | null>(null);
	let picked = $state<number[]>([]);
	let noteOpen = $state(false);
	let note = $state('');
	let busy = $state(false);

	const pending = $derived(peoplePicker.pending);

	$effect(() => {
		const now = pending;
		if (!now) return;
		people = null;
		picked = [...(now.picked ?? [])];
		noteOpen = false;
		note = '';
		busy = false;
		api
			.choosable(now.purpose, now.titleId)
			.then((r) => {
				if (peoplePicker.pending === now) people = r.people;
			})
			.catch(() => {
				if (peoplePicker.pending === now) people = [];
			});
	});

	backCloses(
		() => !!pending,
		() => peoplePicker.answer(null)
	);

	function onKey(e: KeyboardEvent) {
		if (pending && e.key === 'Escape') {
			e.preventDefault();
			peoplePicker.answer(null);
		}
	}

	function confirm() {
		const trimmed = note.trim();
		peoplePicker.answer({ ids: picked, note: noteOpen && trimmed ? trimmed : null });
	}
</script>

<svelte:window onkeydown={onKey} />

{#if pending}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		class="scrim"
		transition:fade={{ duration: 120 }}
		onclick={() => peoplePicker.answer(null)}
	></div>

	<div
		class="dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="people-title"
		transition:fly={{ y: 16, duration: 180 }}
	>
		<h2 id="people-title">{pending.heading}</h2>

		<div class="list">
			<PeopleList
				{people}
				bind:picked
				label="People"
				sentLabel={pending.purpose === 'recommend' ? 'Sent' : undefined}
			/>
		</div>

		{#if pending.note}
			{#if noteOpen}
				<label class="note">
					<span class="faint">Note</span>
					<textarea
						class="input"
						rows="2"
						maxlength={NOTE_MAX}
						placeholder="Why they'll like it"
						bind:value={note}></textarea>
				</label>
			{:else}
				<button class="btn ghost small add-note" onclick={() => (noteOpen = true)}>
					Add a note
				</button>
			{/if}
		{/if}

		<div class="actions">
			<button class="btn ghost" onclick={() => peoplePicker.answer(null)}>Cancel</button>
			<button
				class="btn primary"
				disabled={busy || (!pending.allowNone && picked.length === 0)}
				onclick={confirm}
			>
				{pending.confirm}
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
		width: min(420px, calc(100vw - 28px));
		max-height: calc(100dvh - 40px);
		overflow-y: auto;
		padding: 22px;
		border-radius: var(--radius-lg);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
	}

	/* A phone gets a sheet along the bottom, in reach of a thumb. */
	@media (max-width: 640px) {
		.dialog {
			top: auto;
			bottom: 0;
			transform: translateX(-50%);
			width: 100vw;
			max-height: 85dvh;
			padding-bottom: calc(22px + env(safe-area-inset-bottom));
			border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		}
	}

	h2 {
		font-size: 1.2rem;
		overflow-wrap: anywhere;
	}

	.list {
		margin: 16px 0 12px;
	}

	.add-note {
		margin-bottom: 12px;
	}

	.note {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 14px;
		font-size: 0.8rem;
	}

	.note textarea {
		resize: vertical;
		font: inherit;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
	}
</style>
