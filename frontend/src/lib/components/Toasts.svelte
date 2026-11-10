<script lang="ts">
	import { fly } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import Mascot from './Mascot.svelte';
	import { toasts, type Toast } from '#lib/toast.svelte';

	async function act(toast: Toast) {
		toasts.dismiss(toast.id);
		await toast.action?.run();
	}
</script>

<div class="stack" role="region" aria-label="Notifications">
	{#each toasts.items as toast (toast.id)}
		<div
			class="toast"
			class:cheer={toast.cheer}
			data-tone={toast.tone}
			transition:fly={{ y: 14, duration: 180 }}
		>
			{#if toast.cheer}<span class="pip"><Mascot pose="happy" size={52} /></span>{/if}
			<span class="message">{toast.message}</span>

			{#if toast.action}
				<button class="action" onclick={() => act(toast)}>
					<Icon name="undo" size={14} />
					{toast.action.label}
				</button>
			{/if}

			<button class="close" aria-label="Dismiss" onclick={() => toasts.dismiss(toast.id)}>
				<Icon name="x" size={14} />
			</button>
		</div>
	{/each}
</div>

<style>
	.stack {
		position: fixed;
		left: 50%;
		bottom: calc(18px + env(safe-area-inset-bottom) + var(--toast-lift, 0px));
		transform: translateX(-50%);
		display: flex;
		flex-direction: column;
		gap: 8px;
		z-index: 80;
		pointer-events: none;
		width: min(440px, calc(100vw - 28px));
	}

	.toast {
		pointer-events: auto;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 11px 11px 11px 15px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line-strong);
		background: var(--bg-float);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		font-size: 0.86rem;
	}

	.toast[data-tone='error'] {
		border-color: color-mix(in srgb, var(--bad) 55%, transparent);
		background: color-mix(in srgb, var(--bad) 16%, var(--bg-float));
	}

	/* Pip jumps out over the toast's top edge. */
	.toast.cheer {
		padding-left: 8px;
	}

	.pip {
		flex: none;
		display: block;
		margin: -22px 0 -6px;
	}

	.message {
		flex: 1;
		min-width: 0;
	}

	.action {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 11px;
		border-radius: var(--radius-pill);
		border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
		background: var(--accent-soft);
		color: var(--text);
		font-size: 0.78rem;
		font-weight: 650;
		cursor: pointer;
		flex: none;
	}

	.action:hover {
		background: var(--accent);
		color: var(--accent-ink);
	}

	.close {
		display: grid;
		place-items: center;
		padding: 5px;
		border: none;
		background: none;
		color: var(--text-faint);
		cursor: pointer;
		flex: none;
	}

	.close:hover {
		color: var(--text);
	}

	/* The same width the tab bar appears at, so a toast always clears it. */
	@media (max-width: 760px) {
		.stack {
			bottom: calc(78px + env(safe-area-inset-bottom));
		}
	}
</style>
