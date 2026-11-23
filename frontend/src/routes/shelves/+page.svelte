<script lang="ts">
	import { untrack } from 'svelte';
	import { flip } from 'svelte/animate';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { address } from '#lib/address';
	import { api } from '#lib/api';
	import { keepShelves, lastShelves } from '#lib/bookcase';
	import { shown, type ShelfFilter } from '#lib/books';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import NewShelf from '#lib/components/NewShelf.svelte';
	import ShelfRow from '#lib/components/ShelfRow.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { runtime } from '#lib/format';
	import { shelfMenu } from '#lib/menus';
	import { session } from '#lib/session.svelte';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { Shelf } from '#lib/types';

	// The last answer draws at once, so Back from a shelf lands on the
	// bookcase it left and the zoom has somewhere to go.
	let shelves = $state<Shelf[]>(lastShelves() ?? []);
	let loading = $state(lastShelves() === null);
	let failed = $state<Failure | null>(null);
	// Home's "New shelf" tile lands here with ?new=1.
	let creating = $state(page.url.searchParams.has('new'));
	let arranging = $state(false);

	const FILTERS: { value: ShelfFilter; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'watchlist', label: 'Watchlists' },
		{ value: 'manual', label: 'Hand-picked' },
		{ value: 'smart', label: 'Smart' }
	];

	// Which shelves show lives in the address, so Back from a shelf comes
	// back to the same view.
	const asked = $derived(address().searchParams);
	const filter = $derived(
		(FILTERS.find((f) => f.value === asked.get('show'))?.value ?? 'all') as ShelfFilter
	);

	function view(next: { show?: ShelfFilter }) {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const params = new URLSearchParams(untrack(() => address().search));
		for (const [key, value] of Object.entries(next)) {
			if (!value || value === 'all') params.delete(key);
			else params.set(key, value);
		}
		const search = params.size ? `?${params}` : '';
		void goto(`/shelves${search}`, { shallow: true, replace: true, state: page.state });
	}

	const counts = $derived.by(() => {
		const n: Record<ShelfFilter, number> = {
			all: shelves.length,
			watchlist: 0,
			manual: 0,
			smart: 0
		};
		for (const s of shelves) n[s.kind]++;
		return n;
	});
	/** Arranging is every shelf. */
	const visible = $derived(arranging ? shelves : shelves.filter((s) => shown(s, filter)));
	const toGo = $derived(
		shelves.filter((s) => s.kind === 'watchlist').reduce((n, s) => n + (s.minutes?.left ?? 0), 0)
	);

	$effect(() => {
		if (session.authenticated) load();
	});

	function load() {
		failed = null;
		void api
			.shelves()
			.then((r) => {
				shelves = r.shelves;
				keepShelves(r.shelves);
			})
			.catch((e) => {
				if (!shelves.length)
					failed = failure(e, { title: 'Could not load shelves', hint: 'Try again in a minute.' });
			})
			.finally(() => (loading = false));
	}

	function arrange() {
		if (!arranging && filter !== 'all') view({ show: 'all' });
		arranging = !arranging;
		if (!arranging) toasts.push('Order saved');
	}

	async function saveOrder() {
		keepShelves(shelves);
		try {
			await api.reorderShelves(shelves.map((s) => s.id));
		} catch (e) {
			toasts.error(e);
		}
	}

	async function created(shelf: Shelf) {
		creating = false;
		await goto(`/shelves/${shelf.id}${shelf.kind === 'smart' ? '?edit=1' : ''}`);
	}

	/** Shelves sliding to their places as the view changes; arranging slides its own. */
	const slide = $derived({ duration: arranging ? 0 : 560, easing: settle });
	function settle(t: number) {
		// A little past, then back: the shelf lands.
		const s = 1.15;
		return 1 + (s + 1) * (t - 1) ** 3 + s * (t - 1) ** 2;
	}

	/** A shelf filtered out leaves the flow at once, so the rest slide up while it fades. */
	function leave(node: HTMLElement) {
		const { offsetTop: top, offsetLeft: left, offsetWidth: width } = node;
		Object.assign(node.style, {
			position: 'absolute',
			top: `${top}px`,
			left: `${left}px`,
			width: `${width}px`
		});
		return {
			duration: 300,
			css: (t: number, u: number) =>
				`opacity: ${t}; transform: scale(${0.96 + 0.04 * t}); filter: blur(${u * 6}px)`
		};
	}

	function rise(_node: Element, { delay = 0 } = {}) {
		return {
			delay,
			duration: 420,
			css: (t: number, u: number) =>
				`opacity: ${t}; transform: translateY(${u * 22}px) scale(${0.97 + 0.03 * t}); filter: blur(${u * 6}px)`
		};
	}
</script>

<svelte:head>
	<title>Shelves · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<div>
			<h1>Shelves</h1>
			{#if shelves.length}
				<p class="sub">Your library, grouped your way.</p>
				{#if toGo > 0}
					<p class="to-go">
						<Icon name="clock" size={15} /><span
							><b>{runtime(toGo)}</b> left across your watchlists</span
						>
					</p>
				{/if}
			{/if}
		</div>
		<div class="actions">
			{#if shelves.length > 1}
				<button class="btn" class:primary={arranging} onclick={arrange}>
					{#if arranging}<Icon name="check" size={16} /> Done{:else}<Icon name="grip" size={16} /> Arrange{/if}
				</button>
			{/if}
			{#if !arranging}
				<button class="btn primary" onclick={() => (creating = true)}>
					<Icon name="plus" size={16} /> New shelf
				</button>
			{/if}
		</div>
	</div>

	{#if loading}
		<Spinner label="Loading shelves" />
	{:else if failed}
		<LoadFailed {failed} onretry={load} />
	{:else if shelves.length === 0}
		<Empty title="No shelves yet" hint="Group titles your way, by hand or by rules.">
			<button class="btn primary" onclick={() => (creating = true)}>New shelf</button>
		</Empty>
	{:else}
		{#if shelves.length > 1}
			<div class="toolbar" class:off={arranging} inert={arranging}>
				<div class="segmented" role="group" aria-label="Show">
					{#each FILTERS as f (f.value)}
						{#if f.value === 'all' || counts[f.value]}
							<button class:on={filter === f.value} onclick={() => view({ show: f.value })}>
								{f.label} <span class="n">{counts[f.value]}</span>
							</button>
						{/if}
					{/each}
				</div>
			</div>
		{/if}

		<ol
			class="case"
			class:arranging
			use:sortable={{
				onmove: (from, to) => (shelves = moved(shelves, from, to)),
				onend: saveOrder
			}}
		>
			{#each visible as shelf, i (shelf.id)}
				<li data-sort-item animate:flip={slide} in:rise={{ delay: 120 + i * 40 }} out:leave>
					<ShelfRow {shelf} index={i} link={!arranging} menu={shelfMenu(shelf, load)}>
						{#if arranging}
							<button class="grip" data-sort-handle aria-label="Move {shelf.name}">
								<Icon name="grip" size={16} />
							</button>
						{/if}
					</ShelfRow>
				</li>
			{/each}
			{#if visible.length === 0}
				<li class="none">
					<p class="faint">
						No {FILTERS.find((f) => f.value === filter)?.label.toLowerCase()} yet.
					</p>
				</li>
			{/if}
		</ol>
	{/if}
</div>

{#if creating}
	<NewShelf oncreated={created} onclose={() => (creating = false)} />
{/if}

<style>
	.to-go {
		display: inline-flex;
		align-items: center;
		gap: 9px;
		margin: 12px 0 0;
		padding: 6px 14px 6px 11px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.86rem;
	}

	.to-go b {
		color: var(--text);
		font-variant-numeric: tabular-nums;
	}

	.toolbar {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 18px;
		transition: opacity 300ms var(--ease);
	}

	.toolbar.off {
		opacity: 0.35;
	}

	.toolbar .segmented {
		min-width: 0;
		flex: 0 1 auto;
	}

	.n {
		color: var(--text-faint);
		font-weight: 500;
		font-variant-numeric: tabular-nums;
	}

	/* The bookcase: a glass cabinet with a faint grain on its back wall and
	   light catching its two sides. */
	.case {
		position: relative;
		list-style: none;
		margin: 0;
		padding: 6px 30px 26px;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: linear-gradient(180deg, var(--shine), transparent 30%), var(--bg-elev);
		box-shadow:
			var(--shadow-3),
			inset 0 1px 0 var(--shine);
		backdrop-filter: var(--frost);
		-webkit-backdrop-filter: var(--frost);
	}

	.case::before {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
		pointer-events: none;
		background:
			linear-gradient(
				90deg,
				var(--line),
				transparent 18px,
				transparent calc(100% - 18px),
				var(--line)
			),
			repeating-linear-gradient(90deg, var(--shine) 0 2px, transparent 2px 9px);
		opacity: 0.6;
	}

	.case > li {
		position: relative;
		/* A long name must not set the shelf's width. */
		min-width: 0;
	}

	.case > li:first-child {
		padding-top: 4px;
	}

	.none {
		padding: 40px 0 10px;
		text-align: center;
	}

	.grip {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		padding: 0;
		border: 0;
		border-radius: 8px;
		background: var(--bg-elev-2);
		color: var(--text-dim);
		cursor: grab;
		touch-action: none;
	}

	.grip:active {
		cursor: grabbing;
	}

	/* The shelf being moved lifts off the case. */
	.case :global([data-dragging]) {
		z-index: 10;
		filter: drop-shadow(0 30px 40px rgb(0 0 0 / 0.55));
	}

	@media (max-width: 720px) {
		.case {
			padding: 4px 14px 18px;
			border-radius: var(--radius);
		}
	}
</style>
