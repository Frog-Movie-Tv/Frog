<script lang="ts">
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Notifications from '#lib/components/Notifications.svelte';
	import AccountSettings from '#lib/components/AccountSettings.svelte';
	import ProfileSettings from '#lib/components/ProfileSettings.svelte';
	import AniListConnect from '#lib/components/AniListConnect.svelte';
	import TwoFactor from '#lib/components/TwoFactor.svelte';
	import AppearanceSettings from '#lib/components/settings/AppearanceSettings.svelte';
	import HomeSettings from '#lib/components/settings/HomeSettings.svelte';
	import WatchingSettings from '#lib/components/settings/WatchingSettings.svelte';
	import DataSettings from '#lib/components/settings/DataSettings.svelte';
	import ClearLibrary from '#lib/components/settings/ClearLibrary.svelte';
	import AdvancedSettings from '#lib/components/settings/AdvancedSettings.svelte';
	import InstanceInfo from '#lib/components/settings/InstanceInfo.svelte';
	import { toasts } from '#lib/toast.svelte';

	// The page is the frame: which group is open, and the way back from
	// AniList's sign-in. Each group's settings, and their state, live in its
	// own component under lib/components/settings.

	/** One page per group, each with its own address, so a link can open it. */
	const TABS: { id: string; label: string; icon: IconName }[] = [
		{ id: 'account', label: 'Account', icon: 'user' },
		{ id: 'appearance', label: 'Appearance', icon: 'palette' },
		{ id: 'home', label: 'Home and menus', icon: 'home' },
		{ id: 'watching', label: 'Watching', icon: 'tv' },
		{ id: 'connections', label: 'Connections', icon: 'plug' },
		{ id: 'data', label: 'Your data', icon: 'database' }
	];
	/** Older links that point inside a group. */
	const ALIASES: Record<string, string> = { 'this-instance': 'data' };
	const tab = $derived.by(() => {
		const hash = page.url.hash.replace('#', '');
		const id = ALIASES[hash] ?? hash;
		return TABS.some((t) => t.id === id) ? id : 'account';
	});

	// On a phone the groups scroll sideways; keep the open one in sight.
	let tabBar = $state<HTMLElement | null>(null);
	$effect(() => {
		void tab;
		tabBar
			?.querySelector('a.on')
			?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
	});

	// Back from AniList's sign-in: say how it went, once, then tidy the
	// address in place. A real navigation here re-ran this and looped.
	let welcomedBack = false;
	$effect(() => {
		const outcome = page.url.searchParams.get('anilist');
		if (!outcome || welcomedBack) return;
		welcomedBack = true;
		if (outcome === 'connected') toasts.push('AniList connected. The first sync is under way');
		else if (outcome === 'signed-out')
			toasts.push('Sign in to Frog, then connect AniList', 'error');
		else toasts.push('AniList did not finish connecting. Try again', 'error');
		void goto('/settings#connections', { replace: true, shallow: true, state: {} });
	});
</script>

<svelte:head>
	<title>Settings · Frog</title>
</svelte:head>

<div class="page">
	<h1 class="title">Settings</h1>

	<div class="layout">
		<nav class="tabs" aria-label="Settings" bind:this={tabBar}>
			{#each TABS as t (t.id)}
				<a href="#{t.id}" class:on={tab === t.id} aria-current={tab === t.id ? 'page' : undefined}>
					<Icon name={t.icon} size={17} />
					<span>{t.label}</span>
				</a>
			{/each}
		</nav>

		<div class="panel">
			{#if tab === 'account'}
				<section class="block">
					<div class="section-head"><h2>Profile</h2></div>
					<ProfileSettings />
				</section>

				<section class="block">
					<div class="section-head"><h2>Account</h2></div>
					<AccountSettings />
				</section>

				<section class="block">
					<div class="section-head"><h2>Security</h2></div>
					<TwoFactor />
				</section>

				<section class="block">
					<div class="section-head"><h2>Notifications</h2></div>
					<Notifications />
				</section>
			{:else if tab === 'appearance'}
				<AppearanceSettings />
			{:else if tab === 'home'}
				<HomeSettings />
			{:else if tab === 'watching'}
				<WatchingSettings />
			{:else if tab === 'connections'}
				<section class="block">
					<div class="section-head"><h2>Connections</h2></div>
					<p class="faint intro">
						Keep Frog in step with the other places you track what you watch.
					</p>
					<AniListConnect />
				</section>
			{:else if tab === 'data'}
				<DataSettings />
				<ClearLibrary />
				<AdvancedSettings />
				<InstanceInfo />
			{/if}
		</div>
	</div>
</div>

<style>
	.title {
		margin-bottom: 22px;
	}

	.layout {
		display: grid;
		grid-template-columns: 210px minmax(0, 1fr);
		gap: 34px;
		align-items: start;
	}

	.tabs {
		position: sticky;
		top: calc(var(--header-h, 64px) + 16px);
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.tabs a {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		text-decoration: none;
		font-weight: 550;
		white-space: nowrap;
	}

	.tabs a:hover {
		color: var(--text);
		background: var(--bg-elev);
	}

	.tabs a.on {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.tabs a.on :global(.icon) {
		color: var(--accent);
	}

	.intro {
		margin: -6px 0 14px;
		font-size: 0.88rem;
	}

	@media (max-width: 860px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 18px;
		}

		/* A row of chips that scrolls sideways, so every group is one tap away. */
		.tabs {
			position: static;
			flex-direction: row;
			overflow-x: auto;
			gap: 6px;
			min-width: 0;
			padding-bottom: 4px;
			scrollbar-width: none;
		}

		.tabs a {
			padding: 8px 12px;
			border: 1px solid var(--line);
			border-radius: 999px;
		}
	}

	.block {
		margin-bottom: 44px;
	}
</style>
