<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import '../app.css';
	import { goto, onNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import UpdateBanner from '#lib/components/UpdateBanner.svelte';
	import { offline } from '#lib/offline.svelte';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import type { Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();

	const PUBLIC = ['/login'];

	let checked = $state(false);
	let palette: CommandPalette | undefined = $state();

	$effect(() => {
		settings.apply();
	});

	$effect(() => {
		if (session.authenticated) void offline.register();
	});

	$effect(() => {
		if (session.ready) return;
		void session.refresh().then(() => (checked = true));
	});

	// Preferences live on the server, so they load once there is a session.
	$effect(() => {
		if (session.authenticated && !settings.loaded) void settings.load();
	});

	// Route guard. Re-runs on every navigation because `page.url` is reactive.
	$effect(() => {
		if (!session.ready) return;
		const path = page.url.pathname;
		const isPublic = PUBLIC.some((p) => path.startsWith(p));

		if (!session.authenticated && !isPublic) {
			void goto(`/login?next=${encodeURIComponent(path)}`, { replaceState: true });
		} else if (session.authenticated && isPublic) {
			void goto(settings.current.landingPage || '/', { replaceState: true });
		}
	});

	onNavigate((navigation) => {
		if (!document.startViewTransition) return;
		if (settings.current.animations === 'off') return;

		return new Promise((resolve) => {
			try {
				const transition = document.startViewTransition(async () => {
					resolve();
					await navigation.complete;
				});
				// Interrupting one navigation with another rejects these; that is
				// normal and must not surface as an uncaught error.
				transition.finished.catch(() => {});
				transition.ready.catch(() => {});
			} catch {
				resolve();
			}
		});
	});

	const nav = [
		{ href: '/', label: 'Home', icon: 'home' },
		{ href: '/library', label: 'Library', icon: 'grid' },
		{ href: '/search', label: 'Search', icon: 'search' },
		{ href: '/calendar', label: 'Calendar', icon: 'calendar' },
		{ href: '/diary', label: 'Diary', icon: 'book' },
		{ href: '/stats', label: 'Stats', icon: 'chart' },
		{ href: '/settings', label: 'Settings', icon: 'settings' }
	] as const;

	/** The five that fit across a phone. */
	const tabs = $derived(
		nav.filter((n) => ['/', '/library', '/search', '/stats', '/settings'].includes(n.href))
	);

	const current = $derived(page.url.pathname);
	const isActive = (href: string) => (href === '/' ? current === '/' : current.startsWith(href));
	const showChrome = $derived(session.authenticated && !current.startsWith('/login'));

	const version = $derived(session.status?.version ?? '');
	const updated = $derived(
		!!version && settings.loaded && settings.current.lastSeenVersion !== version
	);
</script>

{#if !checked}
	<div class="boot"><Spinner label="Starting Frog" /></div>
{:else}
	{#if showChrome}
		<header class="bar">
			<a class="brand" href="/" aria-label="Frog, home">
				<Logo size={24} word />
			</a>

			<nav class="links">
				{#each nav as link (link.href)}
					<a href={link.href} class:active={isActive(link.href)}>
						{link.label}
						{#if link.href === '/settings' && updated}<i class="dot" aria-label="Updated"></i>{/if}
					</a>
				{/each}
			</nav>

			<div class="right">
				{#if session.status && !session.status.tmdb_configured}
					<a class="pill warn" href="/about">Metadata not connected</a>
				{/if}
				<button class="cmdk" onclick={() => palette?.show()} aria-label="Search and commands">
					<Icon name="search" size={16} />
					<kbd>Ctrl</kbd><kbd>K</kbd>
				</button>
			</div>
		</header>
	{/if}

	<main class:with-chrome={showChrome}>
		{@render children()}
	</main>

	{#if showChrome}
		<nav class="tabs" aria-label="Sections">
			{#each tabs as link (link.href)}
				<a href={link.href} class:active={isActive(link.href)}>
					<Icon name={link.icon} size={20} />
					<small>{link.label}</small>
					{#if link.href === '/settings' && updated}<i class="dot" aria-hidden="true"></i>{/if}
				</a>
			{/each}
		</nav>

		<footer class="attribution">
			<p class="faint">
				Metadata and images from
				<a href="https://www.themoviedb.org" target="_blank" rel="noreferrer noopener">TMDB</a>.
				This product uses the TMDB API but is not endorsed or certified by TMDB.
			</p>
		</footer>
	{/if}
{/if}

{#if session.authenticated}
	<CommandPalette bind:this={palette} />
	<UpdateBanner />
{/if}

<Toasts />

<style>
	.boot {
		min-height: 100dvh;
		display: grid;
		place-items: center;
	}

	.bar {
		position: fixed;
		inset: 0 0 auto 0;
		height: var(--header-h);
		z-index: 40;
		display: flex;
		align-items: center;
		gap: 22px;
		padding: 0 var(--page-pad);
		background: var(--scrim);
		backdrop-filter: saturate(160%) blur(var(--blur));
		border-bottom: 1px solid var(--line);
		/* Keep the blurred bar on its own compositor layer so it stops
		   invalidating the content scrolling underneath it. */
		isolation: isolate;
		backface-visibility: hidden;
		transform: translateZ(0);
		contain: layout style;
	}

	:global(:root[data-blur='0']) .bar,
	:global(:root[data-blur='0']) .tabs {
		backdrop-filter: none;
		background: var(--bg-elev);
	}

	.brand {
		flex: none;
	}

	.links {
		display: flex;
		gap: 4px;
	}

	.links a {
		position: relative;
		padding: 7px 14px;
		border-radius: var(--radius-pill);
		font-size: 0.88rem;
		font-weight: 550;
		color: var(--text-dim);
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.links a:hover,
	.links a.active {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.dot {
		position: absolute;
		top: 5px;
		right: 7px;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--accent);
	}

	.right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.cmdk {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 6px 11px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.85rem;
		cursor: pointer;
		transition: color var(--fast) var(--ease);
	}

	.cmdk:hover {
		color: var(--text);
	}

	kbd {
		padding: 1px 5px;
		border-radius: 4px;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev);
		font-family: var(--font-mono);
		font-size: 0.72rem;
	}

	.pill.warn {
		border-color: color-mix(in srgb, var(--warn) 45%, transparent);
		color: var(--warn);
	}

	.tabs {
		display: none;
	}

	.attribution {
		padding: 0 var(--page-pad) 40px;
		text-align: center;
	}

	.attribution p {
		font-size: 0.72rem;
		max-width: 60ch;
		margin: 0 auto;
	}

	.attribution a {
		text-decoration: underline;
	}

	@media (max-width: 760px) {
		.links,
		.right .cmdk kbd {
			display: none;
		}

		main.with-chrome {
			padding-bottom: 72px;
		}

		.tabs {
			position: fixed;
			inset: auto 0 0 0;
			z-index: 40;
			display: grid;
			grid-auto-flow: column;
			grid-auto-columns: 1fr;
			background: var(--scrim);
			backdrop-filter: saturate(160%) blur(var(--blur));
			border-top: 1px solid var(--line);
			padding-bottom: env(safe-area-inset-bottom);
			backface-visibility: hidden;
			transform: translateZ(0);
			contain: layout style;
		}

		.tabs a {
			position: relative;
			display: flex;
			flex-direction: column;
			align-items: center;
			gap: 2px;
			padding: 9px 0 7px;
			color: var(--text-faint);
			font-size: 1.05rem;
		}

		.tabs a.active {
			color: var(--accent);
		}

		.tabs small {
			font-size: 0.64rem;
			font-weight: 600;
		}

		.tabs .dot {
			top: 6px;
			right: calc(50% - 18px);
		}

		.attribution {
			padding-bottom: 96px;
		}
	}
</style>
