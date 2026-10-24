<script lang="ts">
	import { fly } from 'svelte/transition';
	import { toasts } from '#lib/toast.svelte';
</script>

<div class="stack" role="region" aria-label="Notifications">
	{#each toasts.items as toast (toast.id)}
		<button
			class="toast"
			data-tone={toast.tone}
			transition:fly={{ y: 16, duration: 180 }}
			onclick={() => toasts.dismiss(toast.id)}
		>
			{toast.message}
		</button>
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
		text-align: left;
		padding: 12px 16px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		box-shadow: var(--shadow-3);
		font-size: 0.86rem;
		cursor: pointer;
	}

	.toast[data-tone='error'] {
		border-color: color-mix(in srgb, var(--bad) 55%, transparent);
		background: color-mix(in srgb, var(--bad) 16%, var(--bg-elev-2));
	}

	@media (max-width: 720px) {
		.stack {
			bottom: calc(78px + env(safe-area-inset-bottom));
		}
	}
</style>
