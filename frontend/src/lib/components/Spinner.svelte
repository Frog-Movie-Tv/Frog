<script lang="ts">
	interface Props {
		label?: string;
	}
	let { label = 'Loading' }: Props = $props();
</script>

<div class="wrap" role="status" aria-live="polite">
	<div class="ring"></div>
	<span class="faint">{label}</span>
</div>

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		padding: 56px 0;
		font-size: 0.85rem;
	}

	.ring {
		width: 26px;
		height: 26px;
		border-radius: 50%;
		border: 2px solid var(--line-strong);
		border-top-color: var(--accent);
		animation: spin 700ms linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
