<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import { settings, THEMES } from '#lib/settings.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { LibraryItem, SearchItem } from '#lib/types';

	interface Command {
		id: string;
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
	// which makes jumping to something you already track instant.
	$effect(() => {
		if (!open || mine.length || !session.authenticated) return;
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
		if (localMatches(q).length >= 5) {
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

	const pages: Command[] = [
		{ id: 'p-home', label: 'Home', icon: '⌂', run: () => goto('/') },
		{ id: 'p-library', label: 'Library', icon: '▦', run: () => goto('/library') },
		{ id: 'p-search', label: 'Search', icon: '⌕', run: () => goto('/search') },
		{ id: 'p-stats', label: 'Stats', icon: '◴', run: () => goto('/stats') },
		{ id: 'p-settings', label: 'Settings', icon: '⚙', run: () => goto('/settings') },
		{ id: 'p-new', label: "What's new", icon: '✦', run: () => goto('/whats-new') }
	];

	const actions = $derived<Command[]>([
		...THEMES.map((t) => ({
			id: `t-${t.id}`,
			label: `Theme: ${t.label}`,
			hint: t.hint,
			icon: '◐',
			run: () => {
				settings.set('theme', t.id);
				toasts.push(`${t.label} theme`);
			}
		})),
		{
			id: 'a-spoilers',
			label: settings.current.spoilerProtection
				? 'Turn off spoiler protection'
				: 'Turn on spoiler protection',
			icon: '🙈',
			run: () => settings.set('spoilerProtection', !settings.current.spoilerProtection)
		},
		{
			id: 'a-signout',
			label: 'Sign out',
			icon: '⎋',
			run: async () => {
				await session.logout();
				await goto('/login', { replaceState: true });
			}
		}
	]);

	const results = $derived.by<Command[]>(() => {
		const q = query.trim().toLowerCase();

		const titles: Command[] = localMatches(q).map((i) => ({
			id: `l-${i.id}`,
			label: i.name,
			hint: i.entry ? 'In your library' : undefined,
			poster: i.poster_path,
			run: () => goto(`/title/${i.kind}/${i.tmdb_id}`)
		}));

		const found: Command[] = remote
			.filter((r) => !mine.some((m) => m.kind === r.kind && m.tmdb_id === r.tmdb_id))
			.map((r) => ({
				id: `r-${r.kind}-${r.tmdb_id}`,
				label: r.name,
				hint: `${r.kind === 'tv' ? 'Show' : 'Film'}${r.year ? ` · ${r.year}` : ''} · on TMDB`,
				poster: r.poster_path,
				run: () => goto(`/title/${r.kind}/${r.tmdb_id}`)
			}));

		const matches = (c: Command) => !q || c.label.toLowerCase().includes(q);

		return [...titles, ...found, ...pages.filter(matches), ...actions.filter(matches)].slice(0, 14);
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
			open ? hide() : show();
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
		<div class="field">
			<span class="icon" aria-hidden="true">⌕</span>
			<input
				bind:this={input}
				bind:value={query}
				placeholder="Jump to a title, a page or an action"
				aria-label="Command palette"
				autocomplete="off"
				spellcheck="false"
			/>
			{#if searching}<span class="faint small">searching…</span>{/if}
			<kbd>esc</kbd>
		</div>

		{#if results.length}
			<ul role="listbox" aria-label="Results">
				{#each results as command, i (command.id)}
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
								<span class="glyph" aria-hidden="true">{command.icon ?? '›'}</span>
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

	.field {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--line);
	}

	.field input {
		flex: 1;
		background: none;
		border: none;
		outline: none;
		font-size: 1rem;
	}

	.icon {
		color: var(--text-faint);
		font-size: 1.1rem;
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
		text-align: center;
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
		gap: 16px;
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

	@media (max-width: 720px) {
		footer {
			display: none;
		}
	}
</style>
