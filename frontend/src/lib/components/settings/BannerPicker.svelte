<script lang="ts">
	import { api, img, SEARCH_PAUSE } from '#lib/api';
	import { backCloses } from '#lib/layers.svelte';
	import { portal } from '#lib/portal';
	import { toasts } from '#lib/toast.svelte';
	import type { LibraryItem, SearchItem } from '#lib/types';
	import Icon from '../Icon.svelte';
	import Spinner from '../Spinner.svelte';

	interface Props {
		/** The backdrop in use now, to mark it. */
		current: string | null;
		/** Titles from the library, best rated first. */
		library: LibraryItem[];
		onpick: (path: string) => void;
		onclose: () => void;
	}

	let { current, library, onpick, onclose }: Props = $props();

	let term = $state('');
	let found = $state<SearchItem[]>([]);
	let searching = $state(false);
	/** A title opened for all its pictures. */
	let opened = $state<{ name: string; year: number | null; paths: string[] | null } | null>(null);

	// Back steps out of a title's pictures first, then closes the sheet.
	backCloses(
		() => true,
		() => onclose()
	);
	backCloses(
		() => !!opened,
		() => (opened = null)
	);

	const yours = $derived(library.filter((t) => t.backdrop_path).slice(0, 18));

	$effect(() => {
		const q = term.trim();
		if (!q) {
			found = [];
			searching = false;
			return;
		}
		searching = true;
		const ask = new AbortController();
		const timer = setTimeout(() => {
			api
				.search(q, 1, {}, ask.signal)
				.then((r) => {
					found = r.results.filter((t) => t.tmdb_id && t.backdrop_path).slice(0, 18);
					searching = false;
				})
				.catch((e) => {
					if (ask.signal.aborted) return;
					found = [];
					searching = false;
					toasts.error(e);
				});
		}, SEARCH_PAUSE);
		return () => {
			clearTimeout(timer);
			ask.abort();
		};
	});

	async function open(t: SearchItem) {
		if (!t.tmdb_id) return;
		opened = { name: t.name, year: t.year, paths: null };
		try {
			const { backdrops } = await api.titleBackdrops(t.kind, t.tmdb_id);
			if (opened?.name === t.name) {
				opened.paths = backdrops.length ? backdrops : t.backdrop_path ? [t.backdrop_path] : [];
			}
		} catch (e) {
			toasts.error(e);
			if (opened?.name === t.name) opened.paths = t.backdrop_path ? [t.backdrop_path] : [];
		}
	}
</script>

{#snippet tile(path: string, label: string, sub: string | null, onclick: () => void, wide = false)}
	<button class="tile" class:on={path === current} class:wide aria-label={label} {onclick}>
		<span class="art">
			<img src={img(path, wide ? 'w780' : 'w300')} alt="" loading="lazy" />
			{#if path === current}
				<span class="tick" aria-hidden="true"><Icon name="check" size={14} /></span>
			{/if}
		</span>
		{#if sub !== null}
			<span class="name"
				>{label}{#if sub}<span class="faint">{` · ${sub}`}</span>{/if}</span
			>
		{/if}
	</button>
{/snippet}

<div use:portal>
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" onclick={onclose}></div>
	<div class="card sheet" role="dialog" aria-modal="true" aria-labelledby="banner-picker-title">
		<div class="head">
			{#if opened}
				<button
					class="btn ghost small"
					aria-label="Back to the titles"
					onclick={() => (opened = null)}
				>
					<Icon name="chevron-left" size={16} />
				</button>
				<h3 id="banner-picker-title">
					{opened.name}{#if opened.year}<span class="faint">{` · ${opened.year}`}</span>{/if}
				</h3>
			{:else}
				<h3 id="banner-picker-title">Choose a banner</h3>
			{/if}
			<button class="btn ghost small close" aria-label="Close" onclick={onclose}>
				<Icon name="x" size={16} />
			</button>
		</div>

		{#if opened}
			{#if opened.paths === null}
				<Spinner label="Loading pictures" />
			{:else if opened.paths.length}
				<p class="faint small">Pick the picture for your banner.</p>
				<div class="grid wide">
					{#each opened.paths as path (path)}
						{@render tile(
							path,
							`Use this picture of ${opened.name}`,
							null,
							() => onpick(path),
							true
						)}
					{/each}
				</div>
			{:else}
				<p class="faint small">This title has no pictures to choose from.</p>
			{/if}
		{:else}
			<div class="searchbar">
				<Icon name="search" size={17} />
				<input
					bind:value={term}
					type="search"
					placeholder="Search any film or show"
					aria-label="Search any film or show"
					autocomplete="off"
					spellcheck="false"
				/>
			</div>

			{#if term.trim()}
				{#if found.length}
					<p class="faint small">Choose a title to see all its pictures.</p>
					<div class="grid">
						{#each found as t (`${t.kind}:${t.tmdb_id}`)}
							{@render tile(t.backdrop_path ?? '', t.name, t.year ? String(t.year) : '', () =>
								open(t)
							)}
						{/each}
					</div>
				{:else if searching}
					<Spinner label="Searching" />
				{:else}
					<p class="faint small">Nothing by that name.</p>
				{/if}
			{:else if yours.length}
				<p class="faint small">From the titles you rated highest, or search for any other.</p>
				<div class="grid">
					{#each yours as t (t.id)}
						{@render tile(t.backdrop_path ?? '', t.name, '', () => onpick(t.backdrop_path ?? ''))}
					{/each}
				</div>
			{:else}
				<p class="faint small">Search for a film or show to use one of its pictures.</p>
			{/if}
		{/if}
	</div>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 300;
		background: rgb(0 0 0 / 0.5);
		animation: fade 160ms var(--ease) backwards;
	}

	.sheet {
		position: fixed;
		z-index: 301;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(760px, calc(100vw - 32px));
		height: min(640px, calc(100dvh - 48px));
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px 20px 20px;
		background: var(--bg-float);
		animation: open 220ms var(--spring) backwards;
	}

	@keyframes open {
		from {
			opacity: 0;
			transform: translate(-50%, -48%) scale(0.96);
		}
	}

	@keyframes fade {
		from {
			opacity: 0;
		}
	}

	.head {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 34px;
	}

	.head h3 {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: 1.1rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.searchbar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 14px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		color: var(--text-faint);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.searchbar input {
		flex: 1;
		min-width: 0;
		padding: 10px 0;
		background: none;
		border: none;
		outline: none;
		color: var(--text);
		font: inherit;
	}

	.small {
		margin: 0;
		font-size: 0.85rem;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 200px), 1fr));
		align-content: start;
		gap: 12px;
		overflow-y: auto;
		margin: 0 -4px;
		padding: 4px;
	}

	.grid.wide {
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 320px), 1fr));
	}

	.tile {
		display: grid;
		gap: 6px;
		padding: 0;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.art {
		position: relative;
		display: block;
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		outline: 2px solid transparent;
		outline-offset: 2px;
		transition:
			outline-color var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.tile:hover .art {
		transform: translateY(-2px);
	}

	.tile:focus-visible {
		outline: none;
	}

	.tile:focus-visible .art,
	.tile.on .art {
		outline-color: var(--accent);
	}

	.art img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.tick {
		position: absolute;
		top: 8px;
		right: 8px;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: var(--accent);
		color: var(--accent-ink);
	}

	.name {
		font-size: 0.85rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* A sheet from the foot of a phone. */
	@media (max-width: 600px) {
		.sheet {
			top: auto;
			bottom: 0;
			left: 0;
			transform: none;
			width: 100%;
			height: min(86dvh, 720px);
			border-radius: 22px 22px 0 0;
			padding-bottom: calc(20px + env(safe-area-inset-bottom));
			animation-name: rise;
		}

		@keyframes rise {
			from {
				transform: translateY(24px);
				opacity: 0;
			}
		}

		.grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: 10px;
		}

		.grid.wide {
			grid-template-columns: minmax(0, 1fr);
		}
	}
</style>
