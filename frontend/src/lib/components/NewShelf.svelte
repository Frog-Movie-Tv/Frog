<script lang="ts">
	import { fade, fly } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';
	import type { Shelf } from '#lib/types';

	interface Props {
		oncreated: (shelf: Shelf) => void;
		onclose: () => void;
		/** Put this title on the new shelf straight away. */
		/** A title to put on the new shelf, by Frog's id. */
		titleId?: number | null;
	}

	let { oncreated, onclose, titleId = null }: Props = $props();

	let name = $state('');
	// A shelf made from a title page is for that title: hand-picked.
	let kind = $state<Shelf['kind']>('manual');
	let busy = $state(false);
	let input: HTMLInputElement | undefined = $state();

	$effect(() => {
		input?.focus();
	});

	async function create(event: SubmitEvent) {
		event.preventDefault();
		if (!name.trim()) return;
		busy = true;
		try {
			const { shelf } = await api.createShelf({ name: name.trim(), kind });
			if (titleId && kind === 'manual') {
				await api.addToShelf(shelf.id, titleId);
				shelf.count = 1;
			}
			oncreated(shelf);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	function onKey(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" transition:fade={{ duration: 120 }} onclick={onclose}></div>

<div
	class="dialog"
	role="dialog"
	aria-modal="true"
	aria-labelledby="new-shelf-title"
	transition:fly={{ y: 16, duration: 180 }}
>
	<form onsubmit={create}>
		<h2 id="new-shelf-title">New shelf</h2>

		<label class="field">
			<span>Name</span>
			<input
				bind:this={input}
				class="input"
				bind:value={name}
				maxlength="60"
				placeholder="Comfort watches"
			/>
		</label>

		{#if !titleId}
			<div class="kinds" role="radiogroup" aria-label="Kind of shelf">
				<button
					type="button"
					class="kind"
					class:on={kind === 'manual'}
					role="radio"
					aria-checked={kind === 'manual'}
					onclick={() => (kind = 'manual')}
				>
					<Icon name="grip" size={18} />
					<span
						><strong>Hand-picked</strong><small>Add titles yourself, in your own order</small></span
					>
				</button>
				<button
					type="button"
					class="kind"
					class:on={kind === 'smart'}
					role="radio"
					aria-checked={kind === 'smart'}
					onclick={() => (kind = 'smart')}
				>
					<Icon name="sparkle" size={18} />
					<span><strong>Smart</strong><small>Fills itself from rules you set</small></span>
				</button>
			</div>
		{/if}

		<div class="actions">
			<button type="button" class="btn ghost" onclick={onclose}>Cancel</button>
			<button class="btn primary" disabled={busy || !name.trim()}>Create</button>
		</div>
	</form>
</div>

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
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 22px;
		border-radius: var(--radius-lg);
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
	}

	h2 {
		font-size: 1.2rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.kinds {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.kind {
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
	}

	.kind.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
		color: var(--text);
	}

	.kind span {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.kind strong {
		font-size: 0.92rem;
		color: var(--text);
	}

	.kind small {
		font-size: 0.76rem;
		color: var(--text-faint);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
	}
</style>
