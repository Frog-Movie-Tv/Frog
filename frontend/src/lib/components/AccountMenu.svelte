<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { onScreen } from '#lib/onscreen';
	import { goto } from '$app/navigation';
	import Avatar from './Avatar.svelte';
	import Icon, { type IconName } from './Icon.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

	let open = $state(false);
	// Back closes it on a phone, as Esc does on a computer.
	backCloses(
		() => open,
		() => (open = false)
	);
	let root: HTMLDivElement | undefined = $state();

	const name = $derived(session.status?.username ?? '');
	const role = $derived(
		session.status?.role === 'owner'
			? 'Owner'
			: session.status?.role === 'admin'
				? 'Administrator'
				: 'Member'
	);

	const links = $derived([
		{ href: `/people/${encodeURIComponent(name)}`, label: 'Your profile', icon: 'user' },
		{ href: '/people', label: 'People', icon: 'users' },
		{ href: '/settings', label: 'Settings', icon: 'settings' },
		...(session.status?.is_admin
			? [{ href: '/admin', label: 'Administration', icon: 'shield' }]
			: [])
	] as { href: string; label: string; icon: IconName }[]);

	async function signOut(switching: boolean) {
		open = false;
		await session.logout();
		if (!switching) toasts.push('Signed out');
		await goto('/login', { replace: true });
	}
</script>

<svelte:window
	onclick={(e) => {
		if (open && root && !root.contains(e.target as Node)) open = false;
	}}
	onkeydown={(e) => {
		if (open && e.key === 'Escape') open = false;
	}}
/>

<div class="account" bind:this={root}>
	<button
		class="avatar hit"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label="Account: {name}"
		title={name}
		onclick={() => (open = !open)}
	>
		<Avatar src={session.status?.avatar} {name} size={34} />
	</button>

	{#if open}
		<div class="menu" role="menu" aria-label="Account" use:onScreen>
			<div class="who">
				<strong>{name}</strong>
				<span class="faint">{role}</span>
			</div>
			{#each links as link (link.href)}
				<a href={link.href} role="menuitem" onclick={() => (open = false)}>
					<Icon name={link.icon} size={16} />
					{link.label}
				</a>
			{/each}
			<hr />
			<button role="menuitem" onclick={() => signOut(true)}>
				<Icon name="refresh" size={16} />
				Switch account
			</button>
			<button role="menuitem" onclick={() => signOut(false)}>
				<Icon name="logout" size={16} />
				Sign out
			</button>
		</div>
	{/if}
</div>

<style>
	.account {
		position: relative;
		flex: none;
	}

	/* The ring holds the 34px picture exactly, so it sits centred in it. */
	.avatar {
		width: 36px;
		height: 36px;
		padding: 0;
		display: grid;
		place-items: center;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		background: var(--accent-soft);
		cursor: pointer;
	}

	.menu {
		position: absolute;
		z-index: 60;
		right: 0;
		top: calc(100% + 8px);
		min-width: 220px;
		max-width: min(320px, calc(100vw - 20px));
		padding: 6px;
		border-radius: var(--radius);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		display: flex;
		flex-direction: column;
		/* Grows out of what opened it. */
		animation: pop 180ms var(--spring) backwards;
	}

	.who {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 8px 10px 10px;
		min-width: 0;
	}

	/* A long name shortens; the menu keeps its width. */
	.who strong {
		max-width: 240px;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.who .faint {
		font-size: 0.8rem;
	}

	.menu a,
	.menu button {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 10px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text);
		font-size: 0.9rem;
		text-align: left;
		text-decoration: none;
		cursor: pointer;
	}

	.menu a:hover,
	.menu button:hover {
		background: var(--bg-hover);
	}

	hr {
		border: 0;
		border-top: 1px solid var(--line);
		margin: 4px 2px;
	}
</style>
