<script lang="ts">
	import { img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { episodeCode, prettyDate, relativeDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';

	interface Entry {
		kind: 'movie' | 'tv';
		tmdb_id: number;
		title: string;
		poster_path: string | null;
		accent: string | null;
		date: string;
		season_number: number | null;
		episode_number: number | null;
		episode_name: string | null;
		watched: boolean;
	}

	let entries = $state<Entry[]>([]);
	let loading = $state(true);
	let view = $state<'month' | 'agenda'>('month');
	let cursor = $state(new Date());

	$effect(() => {
		if (!session.authenticated) return;
		void load();
	});

	async function load() {
		try {
			const res = await fetch('/api/calendar', { credentials: 'same-origin' });
			if (!res.ok) throw new Error('Could not load the calendar');
			entries = (await res.json()).entries;
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = false;
		}
	}

	const today = new Date().toISOString().slice(0, 10);

	const byDate = $derived.by(() => {
		const map = new Map<string, Entry[]>();
		for (const entry of entries) {
			const list = map.get(entry.date) ?? [];
			list.push(entry);
			map.set(entry.date, list);
		}
		return map;
	});

	/** Six weeks starting on the Monday on or before the first of the month. */
	const weeks = $derived.by(() => {
		const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
		const start = new Date(first);
		const weekday = (first.getDay() + 6) % 7;
		start.setDate(first.getDate() - weekday);

		const out: { date: string; inMonth: boolean }[][] = [];
		const walker = new Date(start);
		for (let w = 0; w < 6; w++) {
			const row: { date: string; inMonth: boolean }[] = [];
			for (let d = 0; d < 7; d++) {
				const iso = `${walker.getFullYear()}-${String(walker.getMonth() + 1).padStart(2, '0')}-${String(walker.getDate()).padStart(2, '0')}`;
				row.push({ date: iso, inMonth: walker.getMonth() === cursor.getMonth() });
				walker.setDate(walker.getDate() + 1);
			}
			out.push(row);
		}
		return out;
	});

	const upcoming = $derived(entries.filter((e) => e.date >= today));

	const monthLabel = $derived(
		cursor.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
	);

	const dayNames = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

	function shift(months: number) {
		cursor = new Date(cursor.getFullYear(), cursor.getMonth() + months, 1);
	}

	let selected = $state<string | null>(null);
	const selectedEntries = $derived(selected ? (byDate.get(selected) ?? []) : []);
</script>

<svelte:head>
	<title>Calendar · Frog</title>
</svelte:head>

<div class="page">
	<div class="section-head">
		<h1>Calendar</h1>
		<div class="switch" role="group" aria-label="View">
			<button class:on={view === 'month'} onclick={() => (view = 'month')}>Month</button>
			<button class:on={view === 'agenda'} onclick={() => (view = 'agenda')}>Agenda</button>
		</div>
	</div>

	{#if loading}
		<Spinner label="Loading the calendar" />
	{:else if !entries.length}
		<Empty
			art="rest"
			title="Nothing scheduled"
			hint="Follow a show and its episodes will appear here as they are announced."
		>
			<a class="btn primary" href="/search">Find a show</a>
		</Empty>
	{:else if view === 'month'}
		<div class="monthbar">
			<button class="nav" aria-label="Previous month" onclick={() => shift(-1)}>
				<Icon name="chevron-left" size={17} />
			</button>
			<strong>{monthLabel}</strong>
			<button class="nav" aria-label="Next month" onclick={() => shift(1)}>
				<Icon name="chevron-right" size={17} />
			</button>
			<button class="btn ghost small" onclick={() => (cursor = new Date())}>Today</button>
		</div>

		<div class="grid" role="grid">
			{#each dayNames as day (day)}
				<span class="dayname faint">{day}</span>
			{/each}

			{#each weeks as week, w (w)}
				{#each week as cell (cell.date)}
					{@const list = byDate.get(cell.date) ?? []}
					<button
						class="cell"
						class:dim={!cell.inMonth}
						class:today={cell.date === today}
						class:has={list.length > 0}
						class:picked={selected === cell.date}
						onclick={() => (selected = selected === cell.date ? null : cell.date)}
					>
						<span class="num">{Number(cell.date.slice(8, 10))}</span>
						{#if list.length}
							<span class="dots">
								{#each list.slice(0, 3) as entry (`${entry.tmdb_id}-${entry.episode_number}`)}
									<i style:background={entry.accent ?? 'var(--accent)'}></i>
								{/each}
								{#if list.length > 3}<small>+{list.length - 3}</small>{/if}
							</span>
						{/if}
					</button>
				{/each}
			{/each}
		</div>

		{#if selected}
			<section class="day">
				<h2>{prettyDate(selected)}</h2>
				{#if selectedEntries.length}
					<ul class="agenda">
						{#each selectedEntries as entry (`${entry.tmdb_id}-${entry.season_number}-${entry.episode_number}`)}
							{@render row(entry)}
						{/each}
					</ul>
				{:else}
					<p class="muted">Nothing on this day.</p>
				{/if}
			</section>
		{/if}
	{:else}
		<ul class="agenda">
			{#each upcoming as entry (`${entry.tmdb_id}-${entry.season_number}-${entry.episode_number}-${entry.date}`)}
				{@render row(entry)}
			{/each}
		</ul>
	{/if}
</div>

{#snippet row(entry: Entry)}
	<li>
		<a href="/title/{entry.kind}/{entry.tmdb_id}">
			{#if entry.poster_path}
				<img src={img(entry.poster_path, 'w92')} alt="" loading="lazy" />
			{:else}
				<span class="no-art"><Icon name="film" size={14} /></span>
			{/if}
			<span class="what">
				<strong>{entry.title}</strong>
				<span class="faint">
					{#if entry.season_number !== null && entry.episode_number !== null}
						{episodeCode(entry.season_number, entry.episode_number)}
						{#if entry.episode_name}· {entry.episode_name}{/if}
					{:else}
						In cinemas
					{/if}
				</span>
			</span>
			{#if entry.watched}
				<span class="seen"><Icon name="check" size={14} /></span>
			{/if}
			<span class="when">{relativeDate(entry.date)}</span>
		</a>
	</li>
{/snippet}

<style>
	.switch {
		display: inline-flex;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		overflow: hidden;
	}

	.switch button {
		padding: 7px 15px;
		border: none;
		background: transparent;
		color: var(--text-dim);
		font-size: 0.84rem;
		font-weight: 600;
		cursor: pointer;
	}

	.switch button.on {
		background: var(--accent);
		color: var(--accent-ink);
	}

	.monthbar {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 16px;
	}

	.monthbar strong {
		min-width: 11ch;
		font-family: var(--font-display);
		font-size: 1.05rem;
	}

	.nav {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		cursor: pointer;
	}

	.nav:hover {
		color: var(--text);
	}

	.btn.small {
		margin-left: auto;
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 5px;
	}

	.dayname {
		text-align: center;
		font-size: 0.7rem;
		font-weight: 650;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		padding-bottom: 4px;
	}

	.cell {
		position: relative;
		aspect-ratio: 1.15;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		justify-content: space-between;
		gap: 4px;
		padding: 7px 8px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		color: var(--text-dim);
		cursor: pointer;
		transition: border-color var(--fast) var(--ease);
	}

	.cell:hover {
		border-color: var(--line-strong);
	}

	.cell.dim {
		opacity: 0.4;
	}

	.cell.has {
		color: var(--text);
	}

	.cell.today {
		border-color: var(--accent);
	}

	.cell.picked {
		background: var(--accent-soft);
		border-color: var(--accent);
	}

	.num {
		font-size: 0.8rem;
		font-variant-numeric: tabular-nums;
	}

	.dots {
		display: flex;
		align-items: center;
		gap: 3px;
		flex-wrap: wrap;
	}

	.dots i {
		width: 7px;
		height: 7px;
		border-radius: 50%;
	}

	.dots small {
		font-size: 0.62rem;
		color: var(--text-faint);
	}

	.day {
		margin-top: 28px;
	}

	.day h2 {
		margin-bottom: 12px;
	}

	.agenda {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.agenda a {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.agenda a:hover {
		border-color: var(--line-strong);
	}

	.agenda img,
	.no-art {
		width: 34px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		object-fit: cover;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-faint);
	}

	.what {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.what strong {
		font-size: 0.9rem;
	}

	.what .faint {
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.seen {
		color: var(--good);
		display: grid;
		place-items: center;
	}

	.when {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--accent);
		flex: none;
	}

	@media (max-width: 620px) {
		.cell {
			aspect-ratio: 1;
			padding: 5px;
		}

		.dots i {
			width: 5px;
			height: 5px;
		}
	}
</style>
