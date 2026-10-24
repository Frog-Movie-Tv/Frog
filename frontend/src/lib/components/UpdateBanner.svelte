<script lang="ts">
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';

	// Shown once per release, then never again: the dot on Settings carries on
	// nagging gently until the notes are actually read.
	let dismissed = $state(false);

	const version = $derived(session.status?.version ?? '');
	const show = $derived(
		!dismissed &&
			!!version &&
			settings.loaded &&
			!!settings.current.lastSeenVersion &&
			settings.current.lastSeenVersion !== version
	);

	// A first-ever run has nothing to announce, so record the version quietly.
	$effect(() => {
		if (version && settings.loaded && !settings.current.lastSeenVersion) {
			settings.set('lastSeenVersion', version);
		}
	});
</script>

{#if show}
	<div class="banner" role="status">
		<img src="/mascots/popcorn.svg" alt="" width="30" height="30" />
		<span>
			<strong>Updated to {version}</strong>
			<span class="faint">Something new happened while you were out.</span>
		</span>
		<a class="btn primary small" href="/whats-new">See what changed</a>
		<button class="close" aria-label="Dismiss" onclick={() => (dismissed = true)}>×</button>
	</div>
{/if}

<style>
	.banner {
		position: fixed;
		z-index: 50;
		top: calc(var(--header-h) + 10px);
		left: 50%;
		transform: translateX(-50%);
		width: min(560px, calc(100vw - 24px));
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 11px 13px;
		border-radius: var(--radius);
		background: var(--bg-elev-2);
		border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
		box-shadow: var(--shadow-3);
	}

	span {
		display: flex;
		flex-direction: column;
		gap: 1px;
		flex: 1;
		min-width: 0;
		font-size: 0.88rem;
	}

	.faint {
		font-size: 0.76rem;
	}

	.btn.small {
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.close {
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.2rem;
		cursor: pointer;
		padding: 0 4px;
	}

	.close:hover {
		color: var(--text);
	}

	@media (max-width: 720px) {
		.banner .faint {
			display: none;
		}
	}
</style>
