<script lang="ts">
	import { page } from '$app/state';
	import Mascot from '#lib/components/Mascot.svelte';

	const missing = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{missing ? 'Not found' : 'Something went wrong'} · Frog</title>
</svelte:head>

<div class="page">
	<section class="lost card">
		<div class="glow" aria-hidden="true"></div>
		<Mascot pose={missing ? 'lost' : 'dizzy'} size={200} />
		{#if missing}
			<h1>Nothing here</h1>
			<p class="muted">This page doesn't exist, or it has moved.</p>
			<div class="actions">
				<a class="btn primary" href="/">Go home</a>
				<a class="btn" href="/search">Search</a>
			</div>
		{:else}
			<h1>Something went wrong</h1>
			<p class="muted">Reload the page to try again.</p>
			<div class="actions">
				<button class="btn primary" onclick={() => location.reload()}>Reload</button>
				<a class="btn" href="/">Go home</a>
			</div>
		{/if}
	</section>
</div>

<style>
	.lost {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		min-height: min(70dvh, 560px);
		justify-content: center;
		padding: 48px 20px 56px;
		overflow: hidden;
		text-align: center;
	}

	/* A pond's light under the pad. */
	.glow {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 420px;
		height: 420px;
		translate: -50% -40%;
		border-radius: 50%;
		background: radial-gradient(
			circle,
			color-mix(in srgb, var(--accent) 22%, transparent),
			transparent 65%
		);
		pointer-events: none;
	}

	.lost :global(.pip) {
		position: relative;
		filter: drop-shadow(0 18px 28px rgba(0, 0, 0, 0.35));
	}

	h1 {
		margin-top: 10px;
		font-size: 1.5rem;
	}

	p {
		max-width: 44ch;
	}

	.actions {
		position: relative;
		margin-top: 14px;
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 10px;
	}
</style>
