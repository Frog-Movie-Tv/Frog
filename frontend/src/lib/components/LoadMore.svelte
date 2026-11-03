<script lang="ts">
	import Spinner from './Spinner.svelte';

	interface Props {
		/** Whether there is anything more to load. */
		more: boolean;
		loading: boolean;
		onmore: () => void;
	}

	let { more, loading, onmore }: Props = $props();

	let sentinel: HTMLDivElement | undefined = $state();

	/*
	 * The next page arrives as the end of the list comes near, so a long list
	 * reads as one. The button stays for keyboards and for a browser that
	 * never reports the end coming into view.
	 */
	$effect(() => {
		if (!sentinel || !more || typeof IntersectionObserver === 'undefined') return;
		const watcher = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting) && !loading) onmore();
			},
			{ rootMargin: '800px 0px' }
		);
		watcher.observe(sentinel);
		return () => watcher.disconnect();
	});
</script>

{#if more}
	<div class="more" bind:this={sentinel}>
		{#if loading}
			<Spinner label="Loading more" />
		{:else}
			<button class="btn" onclick={onmore}>Show more</button>
		{/if}
	</div>
{/if}

<style>
	.more {
		display: flex;
		justify-content: center;
		padding: 28px 0 8px;
	}
</style>
