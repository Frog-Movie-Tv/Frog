<script lang="ts">
	import { titleHref } from '#lib/titles';
	import { ApiError, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { airDay, localDay } from '#lib/day';
	import { clockTime, episodeCode, plural, prettyDate, relativeDate } from '#lib/format';
	import { session } from '#lib/session.svelte';

	interface Entry {
		/** Frog's id for the title. */
		id: number;
		kind: 'movie' | 'tv';
		title: string;
		poster_path: string | null;
		accent: string | null;
		date: string;
		/** The moment, when the time is known: it decides the day here. */
		air_at: string | null;
		season_number: number | null;
		episode_number: number | null;
		episode_name: string | null;
		watched: boolean;
	}

	let entries = $state<Entry[]>([]);
	let loading = $state(true);
	let view = $state<'month' | 'agenda'>('month');
	let cursor = $state(new Date());

	const today = localDay();
	let failed = $state<Failure | null>(null);
	const CANNOT_LOAD = { title: 'Could not load the calendar', hint: 'Try again in a minute.' };
	let selected = $state<string>(today);

	$effect(() => {
		if (!session.authenticated) return;
		void load();
	});

	async function load() {
		loading = true;
		failed = null;
		try {
			const res = await fetch('/api/calendar', { credentials: 'same-origin' });
			if (!res.ok) throw new ApiError('', res.status);
			entries = ((await res.json()).entries as Entry[])
				.map((e) => ({ ...e, date: airDay(e.date, e.air_at) ?? e.date }))
				.sort(
					(a, b) => a.date.localeCompare(b.date) || (a.air_at ?? '').localeCompare(b.air_at ?? '')
				);
			// Open on the next day that has something, if today has nothing.
			if (!entries.some((e) => e.date === today)) {
				const next = entries.find((e) => e.date >= today);
				if (next) {
					selected = next.date;
					// The month it is in, so the day chosen is a day on the page.
					cursor = new Date(`${next.date}T12:00:00`);
				}
			}
		} catch (e) {
			failed = failure(e, CANNOT_LOAD);
		} finally {
			loading = false;
		}
	}

	const byDate = $derived.by(() => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const map = new Map<string, Entry[]>();
		for (const entry of entries) {
			const list = map.get(entry.date) ?? [];
			list.push(entry);
			map.set(entry.date, list);
		}
		return map;
	});

	/** One cell per show per day: three episodes of one show is one line. */
	function collapse(list: Entry[]) {
		const out: { entry: Entry; count: number }[] = [];
		for (const entry of list) {
			const same = out.find((o) => o.entry.id === entry.id);
			if (same) same.count++;
			else out.push({ entry, count: 1 });
		}
		return out;
	}

	const iso = (d: Date) =>
		`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

	/** Six weeks starting on the Monday on or before the first of the month. */
	const weeks = $derived.by(() => {
		const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const start = new Date(first);
		start.setDate(first.getDate() - ((first.getDay() + 6) % 7));

		const out: { date: string; inMonth: boolean }[][] = [];
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- built and read here, never state
		const walker = new Date(start);
		for (let w = 0; w < 6; w++) {
			const row: { date: string; inMonth: boolean }[] = [];
			for (let d = 0; d < 7; d++) {
				row.push({ date: iso(walker), inMonth: walker.getMonth() === cursor.getMonth() });
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
	const selectedEntries = $derived(byDate.get(selected) ?? []);

	function shift(months: number) {
		cursor = new Date(cursor.getFullYear(), cursor.getMonth() + months, 1);
	}

	function goToday() {
		cursor = new Date();
		selected = today;
	}
</script>

<svelte:head>
	<title>Calendar · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<h1>Calendar</h1>
		<div class="segmented" role="group" aria-label="View">
			<button class:on={view === 'month'} onclick={() => (view = 'month')}>Month</button>
			<button class:on={view === 'agenda'} onclick={() => (view = 'agenda')}>Agenda</button>
		</div>
	</div>

	{#if loading}
		<Spinner label="Loading the calendar" />
	{:else if failed}
		<LoadFailed {failed} onretry={load} />
	{:else if !entries.length}
		<Empty
			art="rest"
			title="Nothing scheduled"
			hint="Follow a show and its episodes appear here as they are announced."
		>
			<a class="btn primary" href="/search">Find a show</a>
		</Empty>
	{:else if view === 'month'}
		<div class="layout">
			<div class="month card">
				<div class="monthbar">
					<strong>{monthLabel}</strong>
					<div class="stepper">
						<button aria-label="Previous month" onclick={() => shift(-1)}>
							<Icon name="chevron-left" size={16} />
						</button>
						<button class="today-btn" onclick={goToday}>Today</button>
						<button aria-label="Next month" onclick={() => shift(1)}>
							<Icon name="chevron-right" size={16} />
						</button>
					</div>
				</div>

				<div class="grid" role="grid" aria-label={monthLabel}>
					{#each dayNames as day (day)}
						<span class="dayname faint">{day}</span>
					{/each}

					{#each weeks as week, w (w)}
						{#each week as cell (cell.date)}
							{@const items = collapse(byDate.get(cell.date) ?? [])}
							<button
								class="cell"
								class:dim={!cell.inMonth}
								class:today={cell.date === today}
								class:has={items.length > 0}
								class:picked={selected === cell.date}
								aria-label="{prettyDate(cell.date)}: {items.length
									? plural(items.length, 'title')
									: 'nothing'}"
								onclick={() => (selected = cell.date)}
							>
								<span class="num">{Number(cell.date.slice(8, 10))}</span>

								<!-- Desktop: who is on. Phone: one coloured dot each. -->
								{#if items.length}
									<span class="titles">
										{#each items.slice(0, 2) as { entry, count } (entry.id)}
											<span
												class="mini"
												class:film={entry.kind === 'movie'}
												class:seen={entry.watched}
											>
												{#if entry.poster_path}
													<img src={img(entry.poster_path, 'w92')} alt="" loading="lazy" />
												{/if}
												<span class="t">{entry.title}{count > 1 ? ` ×${count}` : ''}</span>
											</span>
										{/each}
										{#if items.length > 2}<small class="more">+{items.length - 2} more</small>{/if}
									</span>
									<span class="dots">
										{#each items.slice(0, 4) as { entry } (entry.id)}
											<i
												class:film={entry.kind === 'movie'}
												style:background={entry.kind === 'movie'
													? 'transparent'
													: (entry.accent ?? 'var(--accent)')}
												style:border-color={entry.accent ?? 'var(--accent)'}
											></i>
										{/each}
									</span>
								{/if}
							</button>
						{/each}
					{/each}
				</div>

				<ul class="legend faint" aria-label="Legend">
					<li><i class="key ep"></i> Episode</li>
					<li><i class="key film"></i> Film release</li>
					<li><Icon name="check" size={13} /> Watched</li>
					<li><i class="key today-key"></i> Today</li>
				</ul>
			</div>

			<aside class="day card" aria-live="polite">
				<h2>{prettyDate(selected)}</h2>
				{#if relativeDate(selected) !== prettyDate(selected)}
					<p class="faint when">{relativeDate(selected)}</p>
				{/if}
				{#if selectedEntries.length}
					<ul class="agenda list-pane">
						{#each selectedEntries as entry (`${entry.id}-${entry.season_number}-${entry.episode_number}`)}
							{@render row(entry, false)}
						{/each}
					</ul>
				{:else}
					<p class="muted nothing">Nothing on this day.</p>
				{/if}
			</aside>
		</div>
	{:else}
		<ul class="agenda list-pane">
			{#each upcoming as entry (`${entry.id}-${entry.season_number}-${entry.episode_number}-${entry.date}`)}
				{@render row(entry, true)}
			{/each}
		</ul>
	{/if}
</div>

{#snippet row(entry: Entry, showWhen: boolean)}
	<li>
		<a href={titleHref(entry)}>
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
						{#if entry.air_at}
							· {clockTime(entry.air_at)}
						{:else if entry.date >= today && !entry.watched}
							· time TBA
						{/if}
					{:else}
						Film release
					{/if}
				</span>
			</span>
			{#if entry.watched}
				<span class="seen-mark" title="Watched"><Icon name="check" size={14} /></span>
			{/if}
			{#if showWhen}<span class="when-tag">{relativeDate(entry.date)}</span>{/if}
		</a>
	</li>
{/snippet}

<style>
	/* Grid and the chosen day side by side, so the details are never below
	   the fold on a desktop. */
	.layout {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 320px;
		gap: 26px;
		align-items: start;
	}

	.month {
		padding: 16px;
	}

	.monthbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin: 0 2px 14px;
	}

	.monthbar strong {
		font-family: var(--font-display);
		font-size: 1.15rem;
		letter-spacing: -0.01em;
	}

	/* Previous, Today, next: one capsule, as everywhere a period is stepped. */
	.stepper {
		display: flex;
		align-items: stretch;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		box-shadow: inset 0 1px 0 var(--shine);
		overflow: hidden;
	}

	.stepper button {
		display: grid;
		place-items: center;
		min-height: 32px;
		padding: 0 11px;
		border: 0;
		background: none;
		color: var(--text-dim);
		font: inherit;
		font-size: 0.8rem;
		font-weight: 650;
		cursor: pointer;
	}

	.stepper button:hover {
		color: var(--text);
		background: var(--bg-hover);
	}

	.stepper .today-btn {
		border-inline: 1px solid var(--line);
		color: var(--text);
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(7, minmax(0, 1fr));
		gap: 5px;
	}

	.dayname {
		text-align: center;
		font-size: 0.78rem;
		font-weight: 650;
		padding-bottom: 4px;
	}

	/* Days are quiet tiles inside the month's pane: only a day with something
	   on it, and the day picked, stand out. */
	.cell {
		min-height: 96px;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 4px;
		padding: 6px;
		border-radius: var(--radius-sm);
		border: 1px solid transparent;
		background: none;
		color: var(--text-faint);
		text-align: left;
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			border-color var(--fast) var(--ease);
	}

	.cell:hover {
		background: var(--bg-hover);
	}

	.cell.dim {
		opacity: 0.4;
	}

	.cell.has {
		color: var(--text);
		background: var(--bg-elev-2);
		border-color: var(--line);
	}

	.cell.today .num {
		display: inline-grid;
		place-items: center;
		min-width: 22px;
		height: 22px;
		border-radius: var(--radius-pill);
		background: var(--accent);
		color: var(--accent-ink);
		font-weight: 700;
	}

	.cell.picked {
		border-color: var(--accent);
		box-shadow: inset 0 0 0 1px var(--accent);
		background: color-mix(in srgb, var(--accent) 10%, transparent);
	}

	.num {
		align-self: flex-start;
		font-size: 0.78rem;
		font-variant-numeric: tabular-nums;
	}

	.titles {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.mini {
		display: flex;
		align-items: center;
		gap: 5px;
		min-width: 0;
		padding: 2px 4px 2px 2px;
		border-radius: 5px;
		background: var(--bg-elev-2);
		font-size: 0.68rem;
		line-height: 1.2;
	}

	.mini.film {
		outline: 1px dashed var(--line-strong);
		outline-offset: -1px;
	}

	.mini.seen {
		opacity: 0.55;
	}

	.mini img {
		width: 14px;
		height: 21px;
		border-radius: 2px;
		object-fit: cover;
		flex: none;
	}

	.mini .t {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.more {
		font-size: 0.64rem;
		color: var(--text-faint);
		padding-left: 2px;
	}

	.dots {
		display: none;
		gap: 3px;
		flex-wrap: wrap;
	}

	.dots i {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		border: 1.5px solid transparent;
	}

	.legend {
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 18px;
		margin: 14px 0 0;
		padding: 0;
		font-size: 0.76rem;
	}

	.legend li {
		display: inline-flex;
		align-items: center;
		gap: 7px;
	}

	.key {
		width: 12px;
		height: 12px;
		border-radius: 3px;
		background: var(--bg-elev-2);
		border: 1px solid var(--line-strong);
	}

	.key.ep {
		background: var(--accent);
		border-color: var(--accent);
		border-radius: 50%;
		width: 8px;
		height: 8px;
	}

	.key.film {
		border-style: dashed;
	}

	.key.today-key {
		background: var(--accent);
		border-color: var(--accent);
		border-radius: var(--radius-pill);
		width: 16px;
	}

	.day {
		position: sticky;
		top: calc(var(--header-h) + 16px);
		padding: 18px;
		max-height: calc(100dvh - var(--header-h) - 40px);
		overflow-y: auto;
	}

	.day h2 {
		font-size: 1.05rem;
	}

	.when {
		font-size: 0.8rem;
		margin: 2px 0 14px;
	}

	.nothing {
		font-size: 0.88rem;
	}

	/* Grouped rows with hairlines between them, as a list of the same kind
	   of thing is everywhere else. */
	.agenda {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.day .agenda {
		background: var(--bg-elev-2);
		box-shadow: none;
	}

	.agenda a {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		transition: background var(--fast) var(--ease);
	}

	.agenda a:hover {
		background: var(--bg-hover);
	}

	.agenda img,
	.no-art {
		width: 32px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		object-fit: cover;
		display: grid;
		place-items: center;
		background: var(--bg-elev);
		color: var(--text-faint);
		flex: none;
	}

	.what {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.what strong {
		font-size: 0.88rem;
	}

	.what .faint {
		font-size: 0.75rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.seen-mark {
		color: var(--good);
		display: grid;
	}

	.when-tag {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--accent);
		flex: none;
	}

	/* Below a desktop the details sit under the grid and cells show dots. */
	@media (max-width: 1000px) {
		.layout {
			grid-template-columns: 1fr;
		}

		.day {
			position: static;
			max-height: none;
		}
	}

	@media (max-width: 720px) {
		.cell {
			min-height: 0;
			aspect-ratio: 1;
			padding: 5px;
		}

		.titles {
			display: none;
		}

		.dots {
			display: flex;
		}
	}
</style>
