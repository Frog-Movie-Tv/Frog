<script lang="ts">
	import Mascot from './Mascot.svelte';

	interface Props {
		label?: string;
	}
	let { label = 'Loading' }: Props = $props();
</script>

<div class="wrap" role="status" aria-live="polite">
	<Mascot pose="hop" size={64} />
	<span class="faint">{label}</span>
</div>

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 56px 0;
		font-size: 0.85rem;
	}
</style>
