<script lang="ts">
	import { sameTitle, titleHref, titleKey } from '#lib/titles';
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import { settings, THEMES } from '#lib/settings.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { STATUS_LABEL, type LibraryItem, type SearchItem } from '#lib/types';

	interface Command {
		id: string;
		group: string;
		label: string;
		hint?: string;
		icon?: string;
		poster?: string | null;
		run: () => void | Promise<void>;
	}

	let open = $state(false);
	let query = $state('');
	let cursor = $state(0);
	let input: HTMLInputElement | undefined = $state();
	let mine = $state<LibraryItem[]>([]);
	let remote = $state<SearchItem[]>([]);
	let searching = $state(false);

	export function show() {
		open = true;
		query = '';
		cursor = 0;
		remote = [];
		queueMicrotask(() => input?.focus());
	}

	function hide() {
		open = false;
	}

	// The whole library is small enough to hold in memory and filter locally,
	// which makes jumping to something you already track instant. Read at each
	// opening: a title added since has to be there.
	$effect(() => {
		if (!open || !session.authenticated) return;
		void api
			.library({ limit: 2000 })
			.then((r) => (mine = r.items))
			.catch(() => {});
	});

	// TMDB is only consulted once the local list runs dry, so the common case
	// costs no network at all.
	$effect(() => {
		const q = query.trim();
		if (!open || q.length < 2) {
			remote = [];
			return;
		}
		if (localMatches(q).length >= 6) {
			remote = [];
			return;
		}
		const timer = setTimeout(async () => {
			searching = true;
			try {
				const res = await api.search(q);
				remote = res.results.slice(0, 6);
			} catch {
				remote = [];
			} finally {
				searching = false;
			}
		}, 320);
		return () => clearTimeout(timer);
	});

	function localMatches(q: string): LibraryItem[] {
		const needle = q.toLowerCase();
		return mine.filter((i) => i.name.toLowerCase().includes(needle)).slice(0, 8);
	}

	const GO = 'Go to';

	const pages: Command[] = [
		{ id: 'p-home', group: GO, label: 'Home', icon: 'home', run: () => goto('/') },
		{ id: 'p-library', group: GO, label: 'Library', icon: 'grid', run: () => goto('/library') },
		{ id: 'p-shelves', group: GO, label: 'Shelves', icon: 'layers', run: () => goto('/shelves') },
		{ id: 'p-search', group: GO, label: 'Search', icon: 'search', run: () => goto('/search') },
		{ id: 'p-cal', group: GO, label: 'Calendar', icon: 'calendar', run: () => goto('/calendar') },
		{ id: 'p-diary', group: GO, label: 'Diary', icon: 'book', run: () => goto('/diary') },
		{ id: 'p-stats', group: GO, label: 'Stats', icon: 'chart', run: () => goto('/stats') },
		{ id: 'p-set', group: GO, label: 'Settings', icon: 'settings', run: () => goto('/settings') },
		{ id: 'p-imp', group: GO, label: 'Import', icon: 'upload', run: () => goto('/import') },
		{ id: 'p-about', group: GO, label: 'About', icon: 'info', run: () => goto('/about') },
		{
			id: 'p-new',
			group: GO,
			label: "What's new",
			icon: 'sparkle',
			run: () => goto('/whats-new')
		}
	];

	const DO = 'Actions';

	const actions = $derived<Command[]>([
		{
			id: 'a-theme',
			group: DO,
			label: 'Toggle theme',
			icon: 'contrast',
			run: () => {
				const order = THEMES.map((t) => t.id);
				const next = order[(order.indexOf(settings.current.theme) + 1) % order.length];
				settings.set('theme', next);
				toasts.push(THEMES.find((t) => t.id === next)?.label ?? 'Theme changed');
			}
		},
		...THEMES.map((t) => ({
			id: `t-${t.id}`,
			group: DO,
			label: `Theme: ${t.label}`,
			icon: 'contrast',
			run: () => settings.set('theme', t.id)
		})),
		{
			id: 'a-import',
			group: DO,
			label: 'Import from another service',
			icon: 'upload',
			run: () => goto('/import')
		},
		{
			id: 'a-export',
			group: DO,
			label: 'Export library as JSON',
			icon: 'download',
			run: () => {
				window.location.href = '/api/export/library.json';
			}
		},
		{
			id: 'a-export-csv',
			group: DO,
			label: 'Export library as CSV',
			icon: 'download',
			run: () => {
				window.location.href = '/api/export/library.csv';
			}
		},
		{
			id: 'a-spoilers',
			group: DO,
			label: settings.current.spoilerProtection
				? 'Turn off spoiler protection'
				: 'Turn on spoiler protection',
			icon: 'eye-off',
			run: () => settings.set('spoilerProtection', !settings.current.spoilerProtection)
		},
		{
			id: 'a-signout',
			group: DO,
			label: 'Sign out',
			icon: 'logout',
			run: async () => {
				await session.logout();
				await goto('/login', { replaceState: true });
			}
		}
	]);

	const results = $derived.by<Command[]>(() => {
		const q = query.trim().toLowerCase();
		const matches = (c: Command) => !q || c.label.toLowerCase().includes(q);

		const titles: Command[] = localMatches(q).map((i) => ({
			id: `l-${i.id}`,
			group: 'Library',
			label: i.name,
			hint: i.entry ? STATUS_LABEL[i.entry.status] : undefined,
			poster: i.poster_path,
			run: () => goto(`/t/${i.id}`)
		}));

		const found: Command[] = remote
			.filter((r) => !mine.some((m) => sameTitle(m, r)))
			.map((r) => ({
				id: `r-${titleKey(r)}`,
				group: 'On TMDB',
				label: r.name,
				hint: `${r.kind === 'tv' ? 'Show' : 'Film'}${r.year ? ` \u00b7 ${r.year}` : ''}`,
				poster: r.poster_path,
				run: () => goto(titleHref(r) ?? '/search')
			}));

		// Your own library first, then where to go, then what to do, and only
		// then the wider catalogue.
		return [...titles, ...pages.filter(matches), ...actions.filter(matches), ...found].slice(0, 16);
	});

	/** Where each group starts, so the list can be labelled as it runs. */
	const headings = $derived.by(() => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const out = new Map<number, string>();
		let last = '';
		results.forEach((c, i) => {
			if (c.group !== last) {
				out.set(i, c.group);
				last = c.group;
			}
		});
		return out;
	});

	$effect(() => {
		// Keep the highlight in range as the list changes under it.
		if (cursor >= results.length) cursor = Math.max(0, results.length - 1);
	});

	async function choose(command: Command | undefined) {
		if (!command) return;
		hide();
		await command.run();
	}

	function onKeydown(event: KeyboardEvent) {
		const mod = event.ctrlKey || event.metaKey;
		if (mod && event.key.toLowerCase() === 'k') {
			event.preventDefault();
			if (open) hide();
			else show();
			return;
		}

		if (!open) {
			// Single-key shortcuts, but never while typing somewhere.
			const target = event.target as HTMLElement | null;
			const typing =
				target &&
				(target.tagName === 'INPUT' ||
					target.tagName === 'TEXTAREA' ||
					target.tagName === 'SELECT' ||
					target.isContentEditable);
			if (typing || mod || event.altKey) return;

			if (event.key === '/') {
				event.preventDefault();
				show();
			}
			if (event.key === 'g') pendingG = true;
			else if (pendingG) {
				const map: Record<string, string> = {
					h: '/',
					l: '/library',
					s: '/search',
					t: '/stats',
					c: '/settings'
				};
				const to = map[event.key];
				pendingG = false;
				if (to) {
					event.preventDefault();
					void goto(to);
				}
			}
			return;
		}

		if (event.key === 'Escape') {
			event.preventDefault();
			hide();
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			cursor = (cursor + 1) % Math.max(1, results.length);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			cursor = (cursor - 1 + results.length) % Math.max(1, results.length);
		} else if (event.key === 'Enter') {
			event.preventDefault();
			void choose(results[cursor]);
		}
	}

	let pendingG = $state(false);
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" onclick={hide}></div>

	<div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
		<div class="query">
			<Icon name="search" size={18} />
			<input
				bind:this={input}
				bind:value={query}
				placeholder="Jump to a title, a page or an action"
				aria-label="Command palette"
				autocomplete="off"
				spellcheck="false"
			/>
			{#if searching}<span class="faint small">searching…</span>{/if}
			<kbd class="esc">esc</kbd>
			<button class="close" aria-label="Close" onclick={hide}><Icon name="x" size={18} /></button>
		</div>

		{#if results.length}
			<ul role="listbox" aria-label="Results">
				{#each results as command, i (command.id)}
					{#if headings.has(i)}
						<li class="group-label" role="presentation">{headings.get(i)}</li>
					{/if}
					<li>
						<button
							class="row"
							class:on={i === cursor}
							role="option"
							aria-selected={i === cursor}
							onmouseenter={() => (cursor = i)}
							onclick={() => choose(command)}
						>
							{#if command.poster}
								<img src={img(command.poster, 'w92')} alt="" loading="lazy" />
							{:else}
								<span class="glyph">
									<Icon name={(command.icon ?? 'chevron-right') as IconName} size={16} />
								</span>
							{/if}
							<span class="label">{command.label}</span>
							{#if command.hint}<span class="faint small">{command.hint}</span>{/if}
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="nothing faint">Nothing matches “{query}”.</p>
		{/if}

		<footer>
			<span><kbd>↑</kbd><kbd>↓</kbd> move</span>
			<span><kbd>↵</kbd> open</span>
			<span><kbd>g</kbd> then <kbd>h</kbd> <kbd>l</kbd> <kbd>s</kbd> <kbd>t</kbd> jump</span>
		</footer>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 90;
		background: rgba(0, 0, 0, 0.5);
		backdrop-filter: blur(2px);
	}

	.palette {
		position: fixed;
		z-index: 91;
		top: 12vh;
		left: 50%;
		transform: translateX(-50%);
		width: min(620px, calc(100vw - 28px));
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius);
		box-shadow: var(--shadow-3);
		overflow: hidden;
	}

	/* Named "query", not "field": app.css has a global .field rule for form
	   labels with flex-direction: column, and sharing the name stacked the
	   icon, the input and the esc badge on top of each other. */
	.query {
		display: flex;
		flex-wrap: nowrap;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--line);
	}

	.query input {
		flex: 1 1 auto;
		/* Without an explicit zero basis the input refuses to shrink, wraps
		   onto its own line and ends up sitting under the icon. */
		min-width: 0;
		width: 100%;
		background: none;
		border: none;
		outline: none;
		font-size: 1rem;
		text-align: left;
		line-height: 1.4;
	}

	.close {
		display: none;
		place-items: center;
		flex: none;
		width: 44px;
		height: 44px;
		margin: -10px -12px -10px 0;
		border: none;
		background: none;
		color: var(--text-dim);
		cursor: pointer;
	}

	.group-label {
		padding: 10px 10px 4px;
		font-size: 0.68rem;
		font-weight: 650;
		letter-spacing: 0.09em;
		text-transform: uppercase;
		color: var(--text-faint);
	}

	.group-label:first-child {
		padding-top: 4px;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 6px;
		max-height: 52vh;
		overflow-y: auto;
	}

	.row {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 9px 10px;
		border: none;
		background: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		text-align: left;
	}

	.row.on {
		background: var(--accent-soft);
	}

	.row img {
		width: 26px;
		border-radius: 4px;
		display: block;
	}

	.glyph {
		width: 26px;
		display: grid;
		place-items: center;
		color: var(--text-faint);
	}

	.label {
		flex: 1;
		font-size: 0.9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.small {
		font-size: 0.74rem;
		flex: none;
	}

	.nothing {
		padding: 26px 18px;
		text-align: center;
		font-size: 0.88rem;
	}

	footer {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
		padding: 9px 16px;
		border-top: 1px solid var(--line);
		font-size: 0.72rem;
		color: var(--text-faint);
	}

	kbd {
		display: inline-block;
		padding: 1px 5px;
		margin: 0 1px;
		border-radius: 4px;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		font-family: var(--font-mono);
		font-size: 0.9em;
	}

	/* No keyboard to speak of: its hints go, and esc becomes a real button. */
	@media (hover: none) and (pointer: coarse) {
		footer,
		.esc {
			display: none;
		}

		.close {
			display: grid;
		}
	}
</style>
