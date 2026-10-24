<script lang="ts">
	import '../app.css';
	import { goto, onNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import Spinner from '#lib/components/Spinner.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';
	import type { Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();

	const PUBLIC = ['/login'];

	let checked = $state(false);

	$effect(() => {
		settings.apply();
	});

	$effect(() => {
		if (session.ready) return;
		void session.refresh().then(() => (checked = true));
	});

	// Route guard. Runs on every navigation because `page.url` is reactive.
	$effect(() => {
		if (!session.ready) return;
		const path = page.url.pathname;
		const isPublic = PUBLIC.some((p) => path.startsWith(p));

		if (!session.authenticated && !isPublic) {
			void goto(`/login?next=${encodeURIComponent(path)}`, { replaceState: true });
		} else if (session.authenticated && isPublic) {
			void goto('/', { replaceState: true });
		}
	});

	// Cross-document style animation between routes, with the shared-element
	// morph from a poster into the title page it opens.
	onNavigate((navigation) => {
		if (!document.startViewTransition) return;
		return new Promise((resolve) => {
			document.startViewTransition(async () => {
				resolve();
				await navigation.complete;
			});
		});
	});

	const nav = [
		{ href: '/', label: 'Library', icon: '▦' },
		{ href: '/search', label: 'Search', icon: '⌕' },
		{ href: '/stats', label: 'Stats', icon: '◴' },
		{ href: '/settings', label: 'Settings', icon: '⚙' }
	];

	const current = $derived(page.url.pathname);
	const isActive = (href: string) => (href === '/' ? current === '/' : current.startsWith(href));
	const showChrome = $derived(session.authenticated && !current.startsWith('/login'));
</script>

<svelte:head>
	<title>Frog</title>
</svelte:head>

{#if !checked}
	<div class="boot"><Spinner label="Starting Frog" /></div>
{:else}
	{#if showChrome}
		<header class="bar">
			<a class="brand" href="/">
				<span class="mark" aria-hidden="true"></span>
				<span class="word">Frog</span>
			</a>

			<nav class="links">
				{#each nav as link (link.href)}
					<a href={link.href} class:active={isActive(link.href)}>{link.label}</a>
				{/each}
			</nav>

			<div class="right">
				{#if session.status && !session.status.tmdb_configured}
					<span class="pill warn" title="Set TMDB_API_KEY in .env and restart">No TMDB key</span>
				{/if}
				<a class="btn ghost small" href="/search" aria-label="Search">⌕</a>
			</div>
		</header>
	{/if}

	<main class:with-chrome={showChrome}>
		{@render children()}
	</main>

	{#if showChrome}
		<nav class="tabs" aria-label="Sections">
			{#each nav as link (link.href)}
				<a href={link.href} class:active={isActive(link.href)}>
					<span aria-hidden="true">{link.icon}</span>
					<small>{link.label}</small>
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
		backdrop-filter: saturate(160%) blur(18px);
		border-bottom: 1px solid var(--line);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 9px;
		font-weight: 700;
		letter-spacing: -0.02em;
	}

	.mark {
		width: 16px;
		height: 16px;
		border-radius: 5px;
		background: linear-gradient(140deg, var(--accent), color-mix(in oklab, var(--accent), #28e1a0));
		box-shadow: 0 0 18px -4px var(--accent);
	}

	.links {
		display: flex;
		gap: 4px;
	}

	.links a {
		padding: 7px 14px;
		border-radius: var(--radius-pill);
		font-size: 0.88rem;
		font-weight: 550;
		color: var(--text-dim);
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.links a:hover {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.links a.active {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.btn.small {
		padding: 7px 13px;
		font-size: 0.95rem;
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

	@media (max-width: 720px) {
		.links,
		.right .btn {
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
			backdrop-filter: saturate(160%) blur(18px);
			border-top: 1px solid var(--line);
			padding-bottom: env(safe-area-inset-bottom);
		}

		.tabs a {
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

		.attribution {
			padding-bottom: 96px;
		}
	}
</style>
