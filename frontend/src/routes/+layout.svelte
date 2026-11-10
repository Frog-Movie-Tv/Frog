<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import '../app.css';
	import { goto, onNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import AccountMenu from '#lib/components/AccountMenu.svelte';
	import PullToRefresh from '#lib/components/PullToRefresh.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import ContextMenu from '#lib/components/ContextMenu.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import UpdateBanner from '#lib/components/UpdateBanner.svelte';
	import WhenWatched from '#lib/components/WhenWatched.svelte';
	import { showCutOffText } from '#lib/cutoff';
	import { offline } from '#lib/offline.svelte';
	import { paletteKey } from '#lib/platform';
	import { session } from '#lib/session.svelte';
	import { MAX_TABS, NAV_PAGES, settings } from '#lib/settings.svelte';
	import type { IconName } from '#lib/components/Icon.svelte';
	import type { Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();

	// Reachable signed out: signing in, and joining with an invite link.
	const PUBLIC = ['/login', '/join', '/reset'];
	/** A reset link may be for someone else, so being signed in does not skip it. */
	const isReset = (path: string) => path.startsWith('/reset');

	let checked = $state(false);
	const shortcut = paletteKey(navigator.platform);
	let palette: CommandPalette | undefined = $state();

	$effect(() => {
		settings.apply();
	});

	$effect(() => showCutOffText());

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

		// Signing in navigates on its own as well; whichever navigation loses
		// is aborted, which is expected and not an error.
		if (!session.authenticated && !isPublic) {
			goto(`/login?next=${encodeURIComponent(path)}`, { replaceState: true }).catch(() => {});
		} else if (session.authenticated && isPublic && !isReset(path)) {
			const next = page.url.searchParams.get('next') ?? '';
			// Only a path on this site; never somewhere a link chose for you.
			const local = next.startsWith('/') && !next.startsWith('//') ? next : '';
			goto(local || settings.current.landingPage || '/', { replaceState: true }).catch(() => {});
		}
	});

	onNavigate((navigation) => {
		if (!document.startViewTransition) return;
		if (settings.current.animations === 'off') return;
		// A step on the same page (a filter, a menu opening) is not a new page.
		if (navigation.from?.url.pathname === navigation.to?.url.pathname) return;

		return new Promise((resolve) => {
			// A page that draws no frames (in the background, or busy) would
			// never start the transition, and the navigation would wait on it.
			if (document.hidden) return resolve();
			const fallback = setTimeout(resolve, 600);
			try {
				const transition = document.startViewTransition(async () => {
					clearTimeout(fallback);
					resolve();
					// Interrupted navigations reject here too, and nothing else
					// would catch it.
					// Never longer than a moment: a frozen picture covers the page.
					await Promise.race([
						navigation.complete.catch(() => {}),
						new Promise((r) => setTimeout(r, 1000))
					]);
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

	type Page = (typeof NAV_PAGES)[number];

	/** Pages in the order chosen in Settings; anything new goes at the end. */
	const ordered = $derived.by(() => {
		const order = settings.current.navOrder;
		const known = order
			.map((href) => NAV_PAGES.find((p) => p.href === href))
			.filter((p): p is Page => !!p);
		return [...known, ...NAV_PAGES.filter((p) => !order.includes(p.href))];
	});

	const nav = $derived(
		ordered.filter((p) => p.fixed || !settings.current.navHidden.includes(p.href))
	);

	/** What fits across a phone; everything else lives under More. */
	const tabs = $derived(
		ordered.filter((p) => settings.current.navTabs.includes(p.href)).slice(0, MAX_TABS)
	);
	const more = $derived([
		...ordered.filter((p) => !tabs.includes(p)),
		{ href: '/people', label: 'People', icon: 'users' },
		{ href: '/import', label: 'Import & export', icon: 'upload' },
		{ href: '/whats-new', label: "What's new", icon: 'sparkle' },
		// Only an administrator can do anything there, so only they see it.
		...(session.status?.is_admin
			? [{ href: '/admin', label: 'Administration', icon: 'shield' }]
			: []),
		{ href: '/about', label: 'About', icon: 'info' }
	]);

	let moreOpen = $state(false);
	backCloses(
		() => moreOpen,
		() => (moreOpen = false)
	);
	const inMore = $derived(more.some((p) => isActive(p.href)));

	const current = $derived(page.url.pathname);

	// Navigating anywhere closes the More sheet.
	$effect(() => {
		void current;
		moreOpen = false;
	});
	const isActive = (href: string) => (href === '/' ? current === '/' : current.startsWith(href));

	/** Escape closes the More sheet, like every other sheet and dialog. */
	function closeMore(event: KeyboardEvent) {
		if (event.key === 'Escape' && moreOpen) moreOpen = false;
	}
	const showChrome = $derived(
		session.authenticated && !current.startsWith('/login') && !isReset(current)
	);

	const version = $derived(session.status?.version ?? '');
	const updated = $derived(
		!!version && settings.loaded && settings.current.lastSeenVersion !== version
	);
</script>

<svelte:window onkeydown={closeMore} />

{#if !checked}
	<div class="boot"><Spinner label="Starting Frog" /></div>
{:else}
	{#if showChrome}
		<header class="bar">
			<a class="brand hit" href="/" aria-label="Frog, home">
				<Logo size={28} word />
			</a>

			<nav class="links" aria-label="Sections">
				{#each nav as link (link.href)}
					<a
						href={link.href}
						class:active={isActive(link.href)}
						aria-current={isActive(link.href) ? 'page' : undefined}
					>
						{link.label}
						{#if link.href === '/settings' && updated}<i class="dot" aria-label="Updated"></i>{/if}
					</a>
				{/each}
			</nav>

			<div class="right">
				{#if session.status && !session.status.tmdb_configured}
					<a class="pill warn" href="/about">Metadata not connected</a>
				{/if}
				<button
					class="find"
					onclick={() => palette?.show()}
					aria-label="Search and commands"
					title="Search and commands ({shortcut})"
				>
					<Icon name="search" size={19} />
					<kbd>{shortcut}</kbd>
				</button>
				<AccountMenu />
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
					<Icon name={link.icon as IconName} size={22} filled={isActive(link.href)} />
					<small>{link.label}</small>
					{#if link.href === '/settings' && updated}<i class="dot" aria-hidden="true"></i>{/if}
				</a>
			{/each}
			<button
				class="more-tab"
				class:active={inMore || moreOpen}
				aria-expanded={moreOpen}
				aria-haspopup="menu"
				onclick={() => (moreOpen = !moreOpen)}
			>
				<Icon name="more" size={22} />
				<small>More</small>
				{#if updated && !tabs.some((t) => t.href === '/settings')}
					<i class="dot" aria-hidden="true"></i>
				{/if}
			</button>
		</nav>

		{#if moreOpen}
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="more-scrim" onclick={() => (moreOpen = false)}></div>
			<div class="more-sheet" role="menu" aria-label="More">
				{#each more as link (link.href)}
					<a
						href={link.href}
						role="menuitem"
						class:active={isActive(link.href)}
						onclick={() => (moreOpen = false)}
					>
						<Icon name={link.icon as IconName} size={19} />
						<span>{link.label}</span>
						{#if link.href === '/settings' && updated}<i class="dot inline" aria-hidden="true"
							></i>{/if}
					</a>
				{/each}
			</div>
		{/if}

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
	<PullToRefresh />
	<CommandPalette bind:this={palette} />
	<UpdateBanner />
	<WhenWatched />
	<ContextMenu />
{/if}

<Toasts />

<style>
	.boot {
		min-height: 100dvh;
		display: grid;
		place-items: center;
	}

	/* The bar is a capsule floating over the page: the logo, the sections,
	   search and you, and nothing else. */
	.bar {
		position: fixed;
		inset: 14px 0 auto;
		width: max-content;
		max-width: calc(100% - 28px);
		height: 58px;
		margin-inline: auto;
		z-index: 40;
		display: flex;
		align-items: center;
		gap: 14px;
		/* The account button (36px) ends the capsule as far from its right
		   edge as from its top and bottom, so it sits in the round end. */
		padding: 0 10px 0 18px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-bar);
		backdrop-filter: var(--frost);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		/* On its own compositor layer, so it does not repaint with the content
		   scrolling underneath it. */
		isolation: isolate;
		backface-visibility: hidden;
		transform: translateZ(0);
		contain: layout style;
	}

	.brand {
		flex: none;
		display: flex;
	}

	/* Eight pages and the account button do not fit every laptop. The links
	   give way (scrolling sideways at worst); search and the account never do. */
	.links {
		display: flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.links::-webkit-scrollbar {
		display: none;
	}

	.links a {
		position: relative;
		flex: none;
		display: flex;
		align-items: center;
		height: 40px;
		padding: 0 16px;
		border-radius: var(--radius-pill);
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-dim);
		transition:
			background var(--fast) var(--ease),
			color var(--fast) var(--ease);
	}

	.links a:hover {
		color: var(--text);
		background: color-mix(in srgb, var(--bg-hover) 55%, transparent);
	}

	/* Where you are: the page's name on a raised capsule. */
	.links a.active {
		color: var(--text);
		background: var(--seg-on);
		box-shadow:
			0 1px 4px rgba(0, 0, 0, 0.2),
			inset 0 1px 0 var(--shine);
	}

	.links a:active {
		transform: scale(0.96);
	}

	.dot {
		position: absolute;
		top: 6px;
		right: 6px;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--accent);
	}

	.right {
		min-width: 0;
		margin-left: 6px;
		flex: 0 1 auto;
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* Search is a round button, as it is in a phone's toolbar. Where there is
	   a keyboard and room, its shortcut sits beside the glass. */
	.find {
		flex: none;
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: var(--bg-elev-2);
		color: var(--text);
		cursor: pointer;
		transition: background var(--fast) var(--ease);
	}

	.find:hover {
		background: var(--bg-hover);
	}

	kbd {
		display: none;
		padding: 2px 7px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		color: var(--text-dim);
		font-family: var(--font-mono);
		font-size: 0.68rem;
	}

	@media (hover: hover) and (pointer: fine) and (min-width: 1101px) {
		.find {
			display: flex;
			align-items: center;
			gap: 8px;
			width: auto;
			padding: 0 8px 0 12px;
			border-radius: var(--radius-pill);
		}

		kbd {
			display: inline-block;
		}
	}

	/* Search and the account never give way, so a phone can always reach them. */
	.pill.warn {
		display: block;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		border-color: color-mix(in srgb, var(--warn) 45%, transparent);
		color: var(--warn);
	}

	.tabs {
		display: none;
	}

	/* The footer: a small matte capsule, centred under the page. */
	.attribution {
		padding: 32px var(--page-pad) 36px;
	}

	.attribution p {
		width: fit-content;
		max-width: min(64ch, 100%);
		margin: 0 auto;
		padding: 10px 20px;
		border: 1px solid var(--line);
		border-radius: 18px;
		background: var(--bg-elev);
		font-size: 0.7rem;
		line-height: 1.5;
		text-align: center;
	}

	.attribution a {
		text-decoration: underline;
	}

	@media (max-width: 1100px) {
		.links a {
			padding: 0 11px;
		}
	}

	@media (max-width: 760px) {
		.links {
			display: none;
		}

		/* On a phone the capsule spans the screen: the logo at one end, search
		   and you at the other. */
		.bar {
			inset: 10px 10px auto;
			width: auto;
			max-width: none;
			height: 54px;
			margin: 0;
			padding-right: 8px;
			justify-content: space-between;
		}

		main.with-chrome {
			padding-bottom: calc(92px + env(safe-area-inset-bottom));
		}

		/* The tab bar floats as a capsule above the bottom edge. */
		.tabs {
			position: fixed;
			inset: auto 12px calc(10px + env(safe-area-inset-bottom));
			z-index: 40;
			display: grid;
			grid-auto-flow: column;
			grid-auto-columns: 1fr;
			overflow: hidden;
			border: 1px solid var(--line);
			border-radius: 30px;
			background: var(--bg-bar);
			backdrop-filter: var(--frost);
			box-shadow:
				inset 0 1px 0 var(--shine),
				var(--shadow-3);
			backface-visibility: hidden;
			transform: translateZ(0);
			contain: layout style;
		}

		.tabs a,
		.more-tab {
			position: relative;
			display: flex;
			flex-direction: column;
			align-items: center;
			gap: 3px;
			padding: 9px 0 8px;
			border: 0;
			background: none;
			color: var(--text-faint);
			cursor: pointer;
		}

		.tabs a.active,
		.tabs .more-tab.active {
			color: var(--text);
		}

		.tabs a.active :global(svg) {
			color: var(--accent);
		}

		.tabs small {
			font-size: 0.66rem;
			font-weight: 600;
		}

		.tabs .dot {
			top: 6px;
			right: calc(50% - 18px);
		}

		.more-scrim {
			position: fixed;
			inset: 0;
			z-index: 38;
			background: var(--scrim);
		}

		.more-sheet {
			overflow-wrap: normal;
			position: fixed;
			z-index: 39;
			left: 12px;
			right: 12px;
			bottom: calc(80px + env(safe-area-inset-bottom));
			display: grid;
			grid-template-columns: 1fr 1fr;
			gap: 6px;
			padding: 10px;
			border-radius: var(--radius-lg);
			background: var(--bg-float);
			border: 1px solid var(--line);
			box-shadow:
				inset 0 1px 0 var(--shine),
				var(--shadow-3);
			transform-origin: bottom center;
			animation: pop 180ms var(--spring) backwards;
		}

		.more-sheet a {
			position: relative;
			display: flex;
			align-items: center;
			gap: 11px;
			padding: 13px 14px;
			border-radius: var(--radius-sm);
			background: var(--bg-elev-2);
			color: var(--text-dim);
			font-size: 0.88rem;
			font-weight: 600;
		}

		.more-sheet a.active {
			color: var(--text);
			background: var(--seg-on);
		}

		.more-sheet a.active :global(svg) {
			color: var(--accent);
		}

		.dot.inline {
			position: static;
			margin-left: auto;
		}

		.attribution {
			padding-bottom: calc(104px + env(safe-area-inset-bottom));
		}
	}
</style>
