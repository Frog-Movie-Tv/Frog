<script lang="ts">
	import { fly } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import { toasts, type Toast } from '#lib/toast.svelte';

	async function act(toast: Toast) {
		toasts.dismiss(toast.id);
		await toast.action?.run();
	}
</script>

<div class="stack" role="region" aria-label="Notifications">
	{#each toasts.items as toast (toast.id)}
		<div class="toast" data-tone={toast.tone} transition:fly={{ y: 14, duration: 180 }}>
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
		bottom: calc(18px + env(safe-area-inset-bottom));
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
		background: var(--bg-elev-2);
		box-shadow: var(--shadow-3);
		font-size: 0.86rem;
	}

	.toast[data-tone='error'] {
		border-color: color-mix(in srgb, var(--bad) 55%, transparent);
		background: color-mix(in srgb, var(--bad) 14%, var(--bg-elev-2));
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

	@media (max-width: 720px) {
		.stack {
			bottom: calc(78px + env(safe-area-inset-bottom));
		}
	}
</style>
