<script lang="ts">
	import { api } from '#lib/api';
	import { topSteps } from '#lib/badge-ladder';
	import { session } from '#lib/session.svelte';
	import type { HeldBadge } from '#lib/types';
	import Icon from '../Icon.svelte';

	/* Only a glance: which badges sit beside your name, and the way to the
	   Badges tab, where every badge is and where they are chosen. */
	let badges = $state<HeldBadge[]>([]);
	let loaded = $state(false);

	api
		.myBadges()
		.then((b) => (badges = b.badges))
		.catch(() => {})
		.finally(() => (loaded = true));

	const beside = $derived(topSteps(badges.filter((b) => b.shown)));
	const href = $derived(`/people/${encodeURIComponent(session.status?.username ?? '')}/badges`);
</script>

{#if loaded && badges.length}
	<a class="setting row" {href}>
		<span class="setting-top">
			<span class="setting-tile" aria-hidden="true"><Icon name="sparkle" size={18} /></span>
			<span class="setting-label">
				<strong>Badges</strong>
				<span class="faint">
					{badges.length}
					{badges.length === 1 ? 'badge' : 'badges'} earned.
					{beside.length
						? 'Choose which sit beside your name on your Badges tab.'
						: 'None beside your name yet.'}
				</span>
			</span>
			<span class="worn" aria-hidden="true">
				{#each beside.slice(0, 3) as b (b.id)}
					<img src={b.small} alt="" width="30" height="30" />
				{/each}
			</span>
			<Icon name="chevron-right" size={18} />
		</span>
	</a>
{/if}

<style>
	.row {
		display: block;
		color: inherit;
		text-decoration: none;
	}

	.row .setting-top {
		align-items: center;
	}

	.row:hover strong {
		text-decoration: underline;
		text-underline-offset: 3px;
	}

	.worn {
		display: inline-flex;
		gap: 4px;
		margin-left: auto;
		flex: none;
	}

	.row :global(svg) {
		flex: none;
		color: var(--text-faint);
	}
</style>
