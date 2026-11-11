<script module lang="ts">
	import type { AiringWeek } from '#lib/types';

	/** The last week shown, so coming Back draws at once. */
	let kept: { key: string; week: AiringWeek } | null = null;
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Select from '#lib/components/Select.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { viewerZone } from '#lib/day';
	import { failure, type Failure } from '#lib/failure';
	import { airTime, episodeCode, plural } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { titleHref } from '#lib/titles';
	import type { AiringDay, AiringFilters, AiringItem } from '#lib/types';

	const CANNOT_LOAD = { title: 'Could not load what is airing', hint: 'Try again in a minute.' };

	let list = $state<'all' | 'mine'>('all');
	let kind = $state<'' | 'shows' | 'anime' | 'films'>('');
	let genre = $state('');
	let term = $state('');
	let search = $state('');
	/** Weeks from this one: -1 is last week. */
	let offset = $state(0);

	let week = $state<AiringWeek | null>(kept?.week ?? null);
	/** Today where the viewer is, as the server counted it. */
	let today = $state<string | null>(kept?.week.today ?? null);
	let loading = $state(!kept);
	let failed = $state<Failure | null>(null);
	/** Days with more read in, by date. */
	let extra = $state<Record<string, AiringItem[]>>({});
	let growing = $state<string | null>(null);
	/** The day chosen in the list; none is today, or the week's first day. */
	let picked = $state<string | null>(null);
	/** Now, a minute at a time, for "in 28m" and what has aired. */
	let now = $state(Date.now());

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 60_000);
		return () => clearInterval(timer);
	});

	const filters = $derived<AiringFilters>({
		list: list === 'mine' ? 'mine' : undefined,
		type: kind || undefined,
		genre: genre || undefined,
		q: search || undefined
	});

	// A pause while typing, so a name is one request rather than one a letter.
	$effect(() => {
		const typed = term.trim();
		const timer = setTimeout(() => (search = typed), 250);
		return () => clearTimeout(timer);
	});

	function addDays(day: string, n: number): string {
		const noon = Date.parse(`${day}T12:00:00Z`) + n * 86_400_000;
		return new Date(noon).toISOString().slice(0, 10);
	}

	$effect(() => {
		if (!session.authenticated) return;
		void load(filters, offset);
	});

	async function load(chosen: AiringFilters, weeks: number) {
		const start = untrack(() => today);
		const from = weeks === 0 || !start ? undefined : addDays(start, weeks * 7);
		const key = JSON.stringify([chosen, from ?? null]);
		if (kept?.key === key) {
			week = kept.week;
			loading = false;
			return;
		}
		loading = true;
		failed = null;
		try {
			const answer = await api.airing(chosen, from);
			// Only the latest choice is drawn, however the answers arrive.
			if (JSON.stringify(filters) !== JSON.stringify(chosen) || offset !== weeks) return;
			week = answer;
			today = answer.today;
			extra = {};
			kept = { key, week: answer };
		} catch (e) {
			failed = failure(e, CANNOT_LOAD);
		} finally {
			loading = false;
		}
	}

	async function showMore(day: AiringDay) {
		growing = day.date;
		try {
			const have = day.items.length + (extra[day.date]?.length ?? 0);
			const next = await api.airingDay(filters, day.date, have);
			extra = { ...extra, [day.date]: [...(extra[day.date] ?? []), ...next.items] };
		} catch {
			// The button stays for another try.
		} finally {
			growing = null;
		}
	}

	const canBack = $derived(!!week && addDays(week.today, (offset - 1) * 7) >= week.first);
	const canForward = $derived(!!week && addDays(week.today, (offset + 1) * 7) < week.last);
	const weekName = $derived(
		offset === 0
			? 'This week'
			: offset === -1
				? 'Last week'
				: offset === 1
					? 'Next week'
					: `In ${offset} weeks`
	);

	const range = $derived.by(() => {
		const days = week?.days ?? [];
		if (!days.length) return '';
		return `${dayDate(days[0].date)} – ${dayDate(days[days.length - 1].date)}`;
	});

	const anyFilter = $derived(list !== 'all' || !!kind || !!genre || !!search);
	const nothing = $derived(!!week && week.days.every((d) => d.total === 0));
	const weekTotal = $derived((week?.days ?? []).reduce((n, d) => n + d.total, 0));

	const KINDS = [
		{ value: '', label: 'All' },
		{ value: 'shows', label: 'Shows' },
		{ value: 'anime', label: 'Anime' },
		{ value: 'films', label: 'Films' }
	] as const;
	const genreOptions = $derived([
		{ value: '', label: 'Every genre' },
		...(week?.genres ?? []).map((g) => ({ value: g, label: g }))
	]);

	function clearFilters() {
		list = 'all';
		kind = '';
		genre = '';
		term = '';
		search = '';
	}

	function dayName(date: string): string {
		if (week && date === week.today) return 'Today';
		if (week && date === addDays(week.today, 1)) return 'Tomorrow';
		if (week && date === addDays(week.today, -1)) return 'Yesterday';
		return new Date(`${date}T12:00:00Z`).toLocaleDateString(undefined, {
			weekday: 'long',
			timeZone: 'UTC'
		});
	}

	function dayDate(date: string): string {
		return new Date(`${date}T12:00:00Z`).toLocaleDateString(undefined, {
			day: 'numeric',
			month: 'short',
			timeZone: 'UTC'
		});
	}

	/** TMDB's stand-in for a name it does not have yet. */
	function named(item: AiringItem): string | null {
		const name = item.episode_name?.trim();
		return name && !/^Episode \d+$/i.test(name) ? name : null;
	}

	/** The day on show: the one picked, else today, else the week's first. */
	const shown = $derived.by(() => {
		const days = week?.days ?? [];
		return (
			days.find((d) => d.date === picked) ??
			days.find((d) => d.date === week?.today) ??
			days[0] ??
			null
		);
	});
	const shownItems = $derived(shown ? [...shown.items, ...(extra[shown.date] ?? [])] : []);

	const PARTS = ['Late night', 'Morning', 'Afternoon', 'Evening', 'Time TBA', 'Films'] as const;

	/** The hour a moment falls at where the viewer is. */
	function hourHere(at: string): number {
		const hour = new Intl.DateTimeFormat('en-GB', {
			hour: '2-digit',
			hourCycle: 'h23',
			timeZone: viewerZone()
		}).format(new Date(at));
		return Number(hour);
	}

	/** The day's shows by part of the day, each part most watched first. */
	const parts = $derived.by(() => {
		const out = PARTS.map((name) => ({ name, items: [] as AiringItem[] }));
		for (const item of shownItems) {
			if (item.kind === 'movie') {
				out[5].items.push(item);
				continue;
			}
			if (!item.air_at) {
				out[4].items.push(item);
				continue;
			}
			const hour = hourHere(item.air_at);
			out[hour < 6 ? 0 : hour < 12 ? 1 : hour < 18 ? 2 : 3].items.push(item);
		}
		return out.filter((p) => p.items.length);
	});

	/** "in 28m", "in 2h 5m", "13h ago", "2d ago". */
	function fromNow(at: string): string {
		const minutes = Math.round((Date.parse(at) - now) / 60_000);
		const span = Math.abs(minutes);
		const text =
			span < 60
				? `${span}m`
				: span < 24 * 60
					? `${Math.floor(span / 60)}h${span % 60 && span < 600 ? ` ${span % 60}m` : ''}`
					: `${Math.round(span / (24 * 60))}d`;
		return minutes >= 0 ? `in ${text}` : `${text} ago`;
	}

	function aired(item: AiringItem): boolean {
		if (item.air_at) return Date.parse(item.air_at) <= now;
		return !!week && item.air_date < week.today;
	}

	/** Out within three hours. */
	function soon(item: AiringItem): boolean {
		if (!item.air_at) return false;
		const left = Date.parse(item.air_at) - now;
		return left > 0 && left < 3 * 3_600_000;
	}
</script>

<svelte:head>
	<title>Airing · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<div>
			<h1>Airing</h1>
			<p class="sub">
				{week?.ready && weekTotal ? `${weekName} · ${plural(weekTotal, 'title')} on air` : weekName}
			</p>
		</div>
		<div class="controls">
			<div class="segmented" role="group" aria-label="Shows">
				<button class="chip" class:on={list === 'all'} onclick={() => (list = 'all')}>All</button>
				<button class="chip" class:on={list === 'mine'} onclick={() => (list = 'mine')}>
					My list
				</button>
			</div>
			<div class="weekbar">
				<button aria-label="Previous week" disabled={!canBack || loading} onclick={() => offset--}>
					<Icon name="chevron-left" size={16} />
				</button>
				<button
					class="range"
					disabled={offset === 0}
					title={offset === 0 ? undefined : 'Back to this week'}
					onclick={() => (offset = 0)}>{range || weekName}</button
				>
				<button aria-label="Next week" disabled={!canForward || loading} onclick={() => offset++}>
					<Icon name="chevron-right" size={16} />
				</button>
			</div>
		</div>
	</div>

	<div class="filters">
		<div class="segmented" role="group" aria-label="Kind">
			{#each KINDS as option (option.value)}
				<button class="chip" class:on={kind === option.value} onclick={() => (kind = option.value)}>
					{option.label}
				</button>
			{/each}
		</div>
		<input
			class="input search"
			type="search"
			placeholder="Search this week"
			aria-label="Search this week"
			bind:value={term}
			enterkeyhint="search"
			onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
		/>

		<Select label="Genre" value={genre} options={genreOptions} onchange={(v) => (genre = v)} />
	</div>

	{#if loading && !week}
		<Spinner label="Loading what is airing" />
	{:else if failed && !week}
		<LoadFailed {failed} onretry={() => load(filters, offset)} />
	{:else if week && !week.ready}
		<Empty
			art="rest"
			title="The week is on its way"
			hint="Every show on air appears here shortly. Check back in a few minutes."
		/>
	{:else if week && nothing}
		{#if anyFilter}
			<Empty art="search" title="Nothing matches" hint="Nothing on this week fits those filters.">
				<button class="btn primary" onclick={clearFilters}>Clear filters</button>
			</Empty>
		{:else}
			<Empty art="rest" title="Nothing on" hint="No episodes are listed for this week yet." />
		{/if}
	{:else if week && shown}
		<div class="layout" class:stale={loading} aria-busy={loading}>
			<nav class="days" aria-label="Days">
				{#each week.days as day (day.date)}
					<button
						class:on={day.date === shown.date}
						aria-current={day.date === shown.date ? 'date' : undefined}
						onclick={() => (picked = day.date)}
					>
						<span class="dname">
							<strong>{dayName(day.date)}</strong>
							<span class="faint">{dayDate(day.date)}</span>
						</span>
						<span class="n">{day.total}</span>
					</button>
				{/each}
			</nav>

			<div class="list">
				{#if !shownItems.length}
					<p class="nothing faint">Nothing on this day.</p>
				{/if}
				{#each parts as part (part.name)}
					<section aria-label={part.name}>
						<h2 class="part">{part.name}</h2>
						<ul>
							{#each part.items as item (`${item.kind}${item.tmdb_id}`)}
								<li>
									<a
										class="item"
										class:aired={aired(item)}
										class:soon={soon(item)}
										href={titleHref({ id: item.id, kind: item.kind, tmdb_id: item.tmdb_id })}
									>
										{#if item.poster_path}
											<img src={img(item.poster_path, 'w92')} alt="" loading="lazy" />
										{:else}
											<span class="no-art"
												><Icon name={item.kind === 'movie' ? 'film' : 'tv'} size={14} /></span
											>
										{/if}
										<span class="what">
											<span class="top">
												<strong class="name">{item.name}</strong>
												{#if item.in_list}<span class="mine">In your list</span>{/if}
											</span>
											{#if item.kind === 'movie'}
												<span class="line faint">
													Film · {item.release_type === 4 ? 'Digital' : 'In cinemas'}
												</span>
											{:else}
												<span class="line faint">
													{episodeCode(item.season, item.episode)}
													{#if item.more}
														· {plural(item.more + 1, 'episode')}
													{:else if named(item)}
														· {named(item)}
													{/if}
												</span>
												<span class="line faint">
													<span class="time" class:tba={!item.air_at}>{airTime(item.air_at)}</span>
													{#if item.air_at}· <span class="rel">{fromNow(item.air_at)}</span>{/if}
													{#if item.network}· {item.network}{/if}
												</span>
											{/if}
										</span>
									</a>
								</li>
							{/each}
						</ul>
					</section>
				{/each}
				{#if shownItems.length < shown.total}
					<button
						class="btn ghost more"
						disabled={growing === shown.date}
						onclick={() => showMore(shown)}
					>
						{growing === shown.date ? 'Loading…' : `Show more (${shown.total - shownItems.length})`}
					</button>
				{/if}
			</div>
		</div>
	{/if}

	<p class="credit faint">
		Air times from
		<a href="https://www.tvmaze.com" target="_blank" rel="noreferrer noopener">TVmaze</a>
		(<a
			href="https://creativecommons.org/licenses/by-sa/4.0/"
			target="_blank"
			rel="noreferrer noopener">CC BY-SA</a
		>) and
		<a href="https://anilist.co" target="_blank" rel="noreferrer noopener">AniList</a>. Everything
		else from TMDB.
	</p>
</div>

<style>
	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
	}

	/* Previous, the week, next: one capsule. */
	.weekbar {
		display: flex;
		align-items: stretch;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
		box-shadow: inset 0 1px 0 var(--shine);
		overflow: hidden;
	}

	.weekbar button {
		display: grid;
		place-items: center;
		padding: 0 12px;
		min-height: 34px;
		border: 0;
		background: none;
		color: var(--text-dim);
		cursor: pointer;
	}

	.weekbar button:hover:not(:disabled) {
		color: var(--text);
		background: var(--bg-hover);
	}

	.weekbar button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.weekbar .range {
		min-width: 8.5rem;
		padding: 0 6px;
		border-inline: 1px solid var(--line);
		color: var(--text);
		font-size: 0.84rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
	}

	.weekbar .range:disabled {
		opacity: 1;
	}

	.filters {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		margin-bottom: 24px;
	}

	.filters .search {
		flex: 1 1 220px;
		min-width: 0;
		max-width: 360px;
	}

	.layout {
		display: grid;
		grid-template-columns: 200px minmax(0, 1fr);
		gap: 28px;
		align-items: start;
		transition: opacity var(--fast) var(--ease);
	}

	.layout.stale {
		opacity: 0.55;
	}

	/* The week down the side; the chosen day carries the accent. */
	.days {
		position: sticky;
		top: calc(var(--header-h) + 16px);
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.days button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		padding: 9px 12px;
		border: 0;
		border-left: 2px solid transparent;
		border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
		background: none;
		color: var(--text-dim);
		text-align: left;
		cursor: pointer;
	}

	.days button:hover {
		color: var(--text);
		background: var(--bg-hover);
	}

	.days button.on {
		border-left-color: var(--accent);
		background: var(--bg-elev);
		color: var(--text);
	}

	.dname {
		display: flex;
		flex-direction: column;
		min-width: 0;
		line-height: 1.3;
	}

	.dname strong {
		font-size: 0.88rem;
		font-weight: 650;
	}

	.dname .faint {
		font-size: 0.74rem;
	}

	.n {
		min-width: 26px;
		padding: 2px 6px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		font-size: 0.72rem;
		font-weight: 650;
		text-align: center;
		font-variant-numeric: tabular-nums;
	}

	.part {
		display: flex;
		align-items: center;
		gap: 12px;
		margin: 0 0 10px;
		color: var(--text-faint);
		font-size: 0.7rem;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	.part::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--line);
	}

	section + section {
		margin-top: 22px;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 8px;
	}

	.item {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
		padding: 8px 12px 8px 8px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		border-left: 3px solid var(--line);
		background: var(--bg-elev);
		transition:
			border-color var(--fast) var(--ease),
			opacity var(--fast) var(--ease);
	}

	.item:hover {
		border-color: var(--line-strong);
	}

	.item.soon {
		border-left-color: var(--accent);
	}

	.item.aired {
		opacity: 0.6;
	}

	.item.aired:hover {
		opacity: 1;
	}

	.item img,
	.no-art {
		width: 40px;
		aspect-ratio: 2 / 3;
		border-radius: 5px;
		object-fit: cover;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
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

	.top {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}

	.name {
		min-width: 0;
		font-size: 0.9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.line {
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.time {
		color: var(--text-dim);
		font-variant-numeric: tabular-nums;
	}

	.time.tba {
		color: var(--text-faint);
	}

	.soon .rel {
		color: var(--accent);
		font-weight: 650;
	}

	.mine {
		flex: none;
		padding: 1px 7px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		color: var(--accent);
		font-size: 0.66rem;
		font-weight: 700;
		white-space: nowrap;
	}

	.more {
		margin-top: 16px;
	}

	.nothing {
		margin: 0;
		font-size: 0.88rem;
	}

	.credit {
		margin: 32px 0 0;
		font-size: 0.76rem;
		text-align: center;
	}

	.credit a {
		text-decoration: underline;
	}

	@media (max-width: 1000px) {
		ul {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	/* A phone: the days become a row that scrolls on its own. */
	@media (max-width: 760px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 18px;
		}

		.days {
			position: static;
			flex-direction: row;
			gap: 6px;
			overflow-x: auto;
			scrollbar-width: none;
			margin-inline: calc(-1 * var(--page-pad));
			padding-inline: var(--page-pad);
		}

		.days button {
			flex: none;
			flex-direction: column;
			align-items: flex-start;
			gap: 4px;
			border: 1px solid var(--line);
			border-radius: var(--radius-sm);
		}

		.days button.on {
			border-color: var(--accent);
		}
	}
</style>
