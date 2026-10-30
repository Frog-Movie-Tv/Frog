<script lang="ts">
	import { titleHref } from '#lib/titles';
	import { api, img } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import { plural, relativeDate } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { AniListEntry, AniListStatus, Kind, SearchItem, SeasonSummary } from '#lib/types';

	/** Below this, an automatic match is shown as one to check. Matches the server. */
	const CONFIDENT = 70;

	type GroupKey = 'check' | 'missing' | 'failed' | 'later' | 'matched' | 'ignored';
	const GROUPS: { key: GroupKey; title: string; hint: string; attention: boolean }[] = [
		{
			key: 'check',
			title: 'Needs a check',
			hint: 'Frog has a likely match for these, but not a certain one. Confirm it or pick another.',
			attention: true
		},
		{
			key: 'missing',
			title: 'Not found',
			hint: 'Nothing on TMDB looks like these yet. They are in your library as AniList has them, and Frog keeps looking. Search under another name to match one now.',
			attention: true
		},
		{
			key: 'failed',
			title: 'Couldn’t apply',
			hint: 'Matched, but not written to your library yet. Frog tries again at the next sync.',
			attention: true
		},
		{
			key: 'later',
			title: 'Coming later',
			hint: 'Not out yet. They wait in your library, and Frog places them once TMDB lists them.',
			attention: false
		},
		{ key: 'matched', title: 'Matched', hint: '', attention: false },
		{
			key: 'ignored',
			title: 'Not in Frog',
			hint: 'You chose to leave these out. They stay on AniList as they are.',
			attention: false
		}
	];

	let status = $state<AniListStatus | null>(null);
	let entries = $state<AniListEntry[]>([]);
	let busy = $state(false);
	/** The group whose list is open, if any. */
	let openGroup = $state<GroupKey | null>(null);
	let filter = $state('');
	/** The entry whose place is being chosen. */
	let choosing = $state<number | null>(null);
	let query = $state('');
	let results = $state<SearchItem[]>([]);
	/** Why a suggested title is not a sure match, by kind and id. */
	let doubts = $state<Record<string, string>>({});
	let season = $state(1);
	let fromEpisode = $state(1);
	/** The show picked for an entry, waiting for its season to be confirmed. */
	let picked = $state<{ tmdb_id: number; name: string; seasons: SeasonSummary[] } | null>(null);
	let picking = $state(false);
	const pickedSeason = $derived(picked?.seasons.find((x) => x.season_number === season) ?? null);
	let leaving = $state(false);

	function groupOf(e: AniListEntry): GroupKey {
		if (e.state === 'ignored') return 'ignored';
		if (e.outcome === 'apply_failed') return 'failed';
		if (e.outcome === 'upcoming') return 'later';
		if (e.outcome === 'no_candidate') return 'missing';
		if (e.state !== 'placed') return 'check';
		if (!e.manual && e.confidence !== null && e.confidence < CONFIDENT) return 'check';
		return 'matched';
	}

	const grouped = $derived.by(() => {
		const out: Record<GroupKey, AniListEntry[]> = {
			check: [],
			missing: [],
			failed: [],
			later: [],
			matched: [],
			ignored: []
		};
		for (const e of entries) out[groupOf(e)].push(e);
		return out;
	});
	const shown = $derived.by(() => {
		if (!openGroup) return [];
		const list = grouped[openGroup];
		const words = filter.trim().toLowerCase();
		if (!words) return list;
		return list.filter(
			(e) =>
				e.name.toLowerCase().includes(words) ||
				(e.title?.name.toLowerCase().includes(words) ?? false)
		);
	});
	const needsYou = $derived(grouped.check.length + grouped.missing.length);
	const expiresSoon = $derived(
		!!status?.expires_at && new Date(status.expires_at).getTime() - Date.now() < 30 * 86_400_000
	);

	async function load() {
		try {
			status = await api.anilist();
			entries = status.connected ? (await api.anilistEntries()).entries : [];
		} catch (e) {
			toasts.error(e);
		}
	}
	load();

	// While a sync runs, look again every few seconds.
	$effect(() => {
		if (!status?.syncing) return;
		const timer = setInterval(load, 3000);
		return () => clearInterval(timer);
	});

	async function connect() {
		busy = true;
		try {
			const { url } = await api.anilistConnect();
			location.href = url;
		} catch (e) {
			toasts.error(e);
			busy = false;
		}
	}

	async function syncNow() {
		busy = true;
		try {
			await api.anilistSync();
			if (status) status.syncing = true;
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	/** Disconnect after a moment, so the toast can take it back. */
	function disconnect() {
		leaving = true;
		const timer = setTimeout(async () => {
			try {
				await api.anilistDisconnect();
				await load();
			} catch (e) {
				toasts.error(e);
			} finally {
				leaving = false;
			}
		}, 7000);
		toasts.undoable('AniList disconnected', () => {
			clearTimeout(timer);
			leaving = false;
		});
	}

	function toggleGroup(key: GroupKey) {
		openGroup = openGroup === key ? null : key;
		filter = '';
		choosing = null;
	}

	/**
	 * The name to search TMDB for: AniList's title without the season, part
	 * and cour markers TMDB keeps out of a show's name.
	 */
	function searchName(name: string): string {
		const cleaned = name
			.replace(/\([^)]*\)|\[[^\]]*\]/g, ' ')
			.replace(/\b(season|part|cour)\s*\d+\b/gi, ' ')
			.replace(/\b\d+(st|nd|rd|th)\s+season\b/gi, ' ')
			.replace(/\s+/g, ' ')
			.replace(/[\s:–-]+$/, '')
			.trim();
		return cleaned || name;
	}

	function startChoosing(entry: AniListEntry) {
		choosing = entry.media_id;
		picked = null;
		query = searchName(entry.name);
		doubts = Object.fromEntries(
			entry.candidates
				.filter((c) => c.problems?.length)
				.map((c) => [c.kind + c.tmdb_id, c.problems!.join(', ')])
		);
		results = entry.candidates.map((c) => ({
			kind: c.kind,
			tmdb_id: c.tmdb_id,
			name: c.name,
			poster_path: c.poster_path,
			backdrop_path: null,
			year: c.year ? Number(c.year) : null,
			overview: null,
			vote: null,
			in_library: null
		}));
		season = entry.title?.season ?? 1;
		fromEpisode = entry.title?.from_episode ?? 1;
	}

	async function search(event: SubmitEvent) {
		event.preventDefault();
		if (!query.trim()) return;
		try {
			const found = await api.search(query.trim());
			doubts = {};
			results = found.results.filter((r) => r.kind === 'movie' || r.kind === 'tv').slice(0, 8);
		} catch (e) {
			toasts.error(e);
		}
	}

	/**
	 * Where an entry most likely starts in a show: straight after the entries
	 * of the same show already placed (a second season follows the first),
	 * or where it already is, or the very beginning.
	 */
	function suggest(entry: AniListEntry, tmdbId: number, seasons: SeasonSummary[]) {
		const numbered = seasons
			.filter((x) => x.season_number >= 1)
			.sort((a, b) => a.season_number - b.season_number);
		const absolute = (s: number, e: number) =>
			numbered.filter((x) => x.season_number < s).reduce((n, x) => n + x.episode_count, 0) + e;
		if (entry.title?.kind === 'tv' && entry.title.tmdb_id === tmdbId) {
			return { season: entry.title.season ?? 1, episode: entry.title.from_episode };
		}
		// An OVA or a special is one of a show's specials far more often than a season.
		if (
			(entry.format === 'OVA' || entry.format === 'SPECIAL') &&
			seasons.some((x) => x.season_number === 0)
		) {
			return { season: 0, episode: 1 };
		}
		let next = 0;
		for (const other of entries) {
			const t = other.title;
			if (other.media_id === entry.media_id || other.state !== 'placed') continue;
			if (t?.kind !== 'tv' || t.tmdb_id !== tmdbId || !other.episodes) continue;
			next = Math.max(next, absolute(t.season ?? 1, t.from_episode - 1) + other.episodes);
		}
		let left = next;
		for (const x of numbered) {
			if (left < x.episode_count) return { season: x.season_number, episode: left + 1 };
			left -= x.episode_count;
		}
		// Past the last season TMDB knows of: the start of the newest one.
		const last = numbered.at(-1);
		return next && last ? { season: last.season_number, episode: 1 } : { season: 1, episode: 1 };
	}

	async function pick(entry: AniListEntry, result: SearchItem) {
		// Placing is onto TMDB's titles: a result without a TMDB id cannot be one.
		const tmdbId = result.tmdb_id;
		if (tmdbId == null) return;
		if (result.kind === 'movie') {
			await choose(entry, 'movie', tmdbId);
			return;
		}
		picking = true;
		try {
			const detail = await api.titleAt('tv', tmdbId);
			// Specials are a season too: an OVA or ONA often lives there.
			const seasons = [...detail.seasons].sort((a, b) =>
				a.season_number === 0 ? 1 : b.season_number === 0 ? -1 : a.season_number - b.season_number
			);
			const spot = suggest(entry, tmdbId, seasons);
			season = spot.season;
			fromEpisode = spot.episode;
			picked = { tmdb_id: tmdbId, name: result.name, seasons };
		} catch (e) {
			toasts.error(e);
		} finally {
			picking = false;
		}
	}

	async function choose(entry: AniListEntry, kind: Kind, tmdbId: number) {
		try {
			await api.anilistChoose(entry.media_id, {
				kind,
				tmdb_id: tmdbId,
				season: kind === 'tv' ? season : undefined,
				from_episode: kind === 'tv' ? fromEpisode : undefined
			});
			choosing = null;
			picked = null;
			toasts.push(`${entry.name} is matched; it syncs in a moment`);
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	/** Keep an automatic match as it is, and never move it again. */
	async function confirm(entry: AniListEntry) {
		const t = entry.title;
		if (!t || t.tmdb_id == null) return;
		try {
			await api.anilistChoose(entry.media_id, {
				kind: t.kind,
				tmdb_id: t.tmdb_id,
				season: t.kind === 'tv' ? (t.season ?? 1) : undefined,
				from_episode: t.kind === 'tv' ? t.from_episode : undefined
			});
			toasts.push(`${entry.name} is confirmed`);
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	async function ignore(entry: AniListEntry) {
		try {
			await api.anilistChoose(entry.media_id, { ignore: true });
			choosing = null;
			await load();
		} catch (e) {
			toasts.error(e);
		}
	}

	function where(entry: AniListEntry): string {
		const t = entry.title;
		if (!t) return '';
		if (t.kind === 'movie') return t.name;
		const part = t.season === 0 ? 'Specials' : `Season ${t.season ?? 1}`;
		return `${t.name} · ${part}${t.from_episode > 1 ? `, from episode ${t.from_episode}` : ''}`;
	}

	const METHODS: Record<string, string> = {
		'TMDB id list': 'Matched by id',
		'TVDB id list': 'Matched by id',
		'IMDb id': 'Matched by id',
		'air date': 'Matched by air date',
		'the season before it': 'Follows the season before it',
		'name and year': 'Matched by name and year',
		'your choice': 'Your choice'
	};

	const STATUSES: Record<string, string> = {
		CURRENT: 'Watching',
		REPEATING: 'Rewatching',
		COMPLETED: 'Completed',
		PLANNING: 'Planning',
		PAUSED: 'Paused',
		DROPPED: 'Dropped'
	};

	/** What AniList says about it: format, length, and how far you are. */
	function about(entry: AniListEntry): string {
		const bits = [entry.format?.replace('_', ' ') ?? 'Anime'];
		if (entry.episodes) bits.push(plural(entry.episodes, 'episode'));
		const state = entry.status ? STATUSES[entry.status] : null;
		if (state) {
			const total = entry.episodes ? ` of ${entry.episodes}` : '';
			bits.push(
				entry.progress && entry.status !== 'COMPLETED'
					? `${state}, ${entry.progress}${total}`
					: state
			);
		}
		if (entry.outcome === 'upcoming' && entry.start_date) {
			bits.push(`Starts ${new Date(entry.start_date).toLocaleDateString()}`);
		}
		return bits.join(' · ');
	}

	/** "just now", "12 min ago", "in 3 min": syncs are minutes apart, not days. */
	function minutes(iso: string | null | undefined): string {
		if (!iso) return '';
		const diff = new Date(iso).getTime() - Date.now();
		if (Number.isNaN(diff)) return '';
		const mins = Math.round(Math.abs(diff) / 60_000);
		if (mins >= 120) return relativeDate(iso);
		if (mins < 1) return diff > 0 ? 'in a moment' : 'just now';
		const span = mins < 60 ? `${mins} min` : `${Math.round(mins / 60)} h`;
		return diff > 0 ? `in ${span}` : `${span} ago`;
	}

	function poster(entry: AniListEntry): string | null {
		return entry.title?.poster_path ?? entry.candidates[0]?.poster_path ?? null;
	}
</script>

{#snippet chooser(entry: AniListEntry)}
	<div class="chooser">
		{#if picked}
			<div class="picked">
				<span class="label">
					<strong>{picked.name}</strong>
					<span class="faint">
						{entry.episodes
							? `${plural(entry.episodes, 'episode')} on AniList, starting at`
							: 'Starting at'}
					</span>
				</span>
				<button class="link" onclick={() => (picked = null)}>Another title</button>
			</div>
			<div class="place">
				<Select
					label="Season"
					value={String(season)}
					options={picked.seasons.map((x) => ({
						value: String(x.season_number),
						label: x.name || (x.season_number === 0 ? 'Specials' : `Season ${x.season_number}`),
						hint: plural(x.episode_count, 'episode')
					}))}
					onchange={(v) => {
						season = Number(v);
						fromEpisode = 1;
					}}
				/>
				<label class="episode">
					<span>Episode</span>
					<input
						class="input"
						type="number"
						min="1"
						max={pickedSeason?.episode_count || undefined}
						bind:value={fromEpisode}
					/>
				</label>
			</div>
			<div class="actions">
				<button class="btn primary small" onclick={() => choose(entry, 'tv', picked!.tmdb_id)}>
					Match here
				</button>
				<button class="btn ghost small" onclick={() => (choosing = null)}>Cancel</button>
			</div>
		{:else}
			<form class="find" onsubmit={search}>
				<input class="input" bind:value={query} aria-label="Search TMDB" placeholder="Search" />
				<button class="btn">Search</button>
			</form>
			{#if results.length}
				<ul class="results">
					{#each results as r (r.kind + r.tmdb_id)}
						<li>
							<button class="result" disabled={picking} onclick={() => pick(entry, r)}>
								{#if img(r.poster_path, 'w92')}
									<img src={img(r.poster_path, 'w92')} alt="" width="34" height="51" />
								{:else}
									<span class="blank"
										><Icon name={r.kind === 'tv' ? 'tv' : 'film'} size={14} /></span
									>
								{/if}
								<span class="label">
									<strong>{r.name}</strong>
									<span class="faint"
										>{r.kind === 'tv' ? 'Show' : 'Film'}{r.year ? ` · ${r.year}` : ''}</span
									>
									{#if doubts[r.kind + r.tmdb_id]}
										<span class="doubt">{doubts[r.kind + r.tmdb_id]}</span>
									{/if}
								</span>
								<Icon name="chevron-right" size={16} />
							</button>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="faint small">Nothing found yet. Try another name.</p>
			{/if}
			<div class="actions">
				{#if entry.state !== 'ignored'}
					<button class="btn ghost small" onclick={() => ignore(entry)}>Not in Frog</button>
				{/if}
				<button class="btn ghost small" onclick={() => (choosing = null)}>Cancel</button>
			</div>
		{/if}
	</div>
{/snippet}

<div class="anilist card">
	<div class="top">
		<div class="label">
			<strong>AniList</strong>
			<span class="faint">
				{#if !status}
					Checking…
				{:else if !status.available}
					Not set up on this Frog.
				{:else if !status.connected || leaving}
					Episodes, statuses and scores, kept the same in both places.
				{:else if status.syncing}
					Syncing with {status.name}…
				{:else if status.synced_at}
					Connected as {status.name} · synced {minutes(status.synced_at)}
				{:else}
					Connected as {status.name}
				{/if}
			</span>
		</div>
		{#if status?.available}
			<div class="buttons">
				{#if !status.connected || leaving}
					<button class="btn primary" disabled={busy || leaving} onclick={connect}>
						<Icon name="link" size={16} /> Connect AniList
					</button>
				{:else}
					<button class="btn" disabled={busy || status.syncing} onclick={syncNow}>
						<Icon name="refresh" size={16} />
						{status.syncing ? 'Syncing…' : 'Sync now'}
					</button>
					<button class="btn ghost" onclick={disconnect}>Disconnect</button>
				{/if}
			</div>
		{/if}
	</div>

	{#if status?.connected && !leaving}
		{#if status.needs_reconnect}
			<p class="problem">
				{status.error ?? 'AniList signed Frog out.'}
				<button class="link" onclick={connect}>Reconnect</button>
			</p>
		{:else if status.error}
			<p class="problem">
				{status.error}
				{#if status.next_sync_at}Next try {minutes(status.next_sync_at)}.{/if}
			</p>
		{:else if status.synced_at && !status.syncing}
			<p class="faint small">
				Changes on AniList arrive by themselves, every {status.interval_minutes ?? 3} minutes or so, even
				while Frog is closed.
			</p>
		{/if}
		{#if expiresSoon}
			<p class="problem">
				AniList asks for a new sign-in once a year. Reconnect before {new Date(
					status.expires_at ?? ''
				).toLocaleDateString()} to keep syncing.
				<button class="link" onclick={connect}>Reconnect</button>
			</p>
		{/if}
		{#if status.summary}
			{@const s = status.summary}
			<p class="faint small">
				{plural(s.entries, 'entry', 'entries')} on AniList · last sync brought {s.to_frog} into Frog and
				sent {s.to_anilist} to AniList{#if s.repaired}&nbsp;· moved {plural(
						s.repaired,
						'match',
						'matches'
					)} to a better place{/if}{#if s.failed}&nbsp;· {plural(s.failed, 'entry', 'entries')} to try
					again{/if}
			</p>
		{/if}

		{#if entries.length}
			{#if needsYou}
				<p class="nudge">
					<Icon name="info" size={15} />
					{plural(needsYou, 'entry', 'entries')}
					{needsYou === 1 ? 'needs' : 'need'} you. Everything else syncs by itself.
				</p>
			{/if}
			<div class="groups" role="tablist" aria-label="AniList entries">
				{#each GROUPS as g (g.key)}
					{#if grouped[g.key].length}
						<button
							class="group-chip"
							class:attention={g.attention}
							class:on={openGroup === g.key}
							role="tab"
							aria-selected={openGroup === g.key}
							onclick={() => toggleGroup(g.key)}
						>
							{g.title}
							<span class="count">{grouped[g.key].length}</span>
						</button>
					{/if}
				{/each}
			</div>

			{#if openGroup}
				{@const g = GROUPS.find((x) => x.key === openGroup)!}
				<div class="panel" role="tabpanel">
					{#if g.hint}<p class="faint small">{g.hint}</p>{/if}
					{#if grouped[g.key].length > 8}
						<input
							class="input filter"
							bind:value={filter}
							placeholder="Filter by name"
							aria-label="Filter entries"
						/>
					{/if}
					<ul class="list">
						{#each shown as entry (entry.media_id)}
							<li>
								<div class="entry">
									{#if img(poster(entry), 'w92')}
										<img
											class="thumb"
											src={img(poster(entry), 'w92')}
											alt=""
											width="36"
											height="54"
										/>
									{:else}
										<span class="thumb blank"><Icon name="tv" size={14} /></span>
									{/if}
									<span class="label">
										<strong>{entry.name}</strong>
										<span class="faint">{about(entry)}</span>
										{#if entry.title && entry.state === 'placed'}
											<span class="where">
												<Icon name="chevron-right" size={12} />
												{where(entry)}
											</span>
										{/if}
										{#if entry.method && entry.state === 'placed' && entry.outcome !== 'apply_failed'}
											{#if METHODS[entry.method]}<span class="how">{METHODS[entry.method]}</span
												>{/if}
										{/if}
										{#if entry.note && (openGroup !== 'matched' || !entry.manual)}
											<span class="note">{entry.note}</span>
										{/if}
										{#if entry.kept && entry.state !== 'placed'}
											<a class="where" href={titleHref(entry.kept)}>
												<Icon name="chevron-right" size={12} />
												In your library as AniList has it
											</a>
										{/if}
										{#if entry.gone}
											<span class="note">Removed from your AniList. Kept in Frog as it is.</span>
										{/if}
									</span>
									{#if choosing !== entry.media_id}
										<div
											class="row-actions"
											class:pair={openGroup === 'check' && entry.state === 'placed'}
										>
											{#if openGroup === 'check' && entry.state === 'placed'}
												<button class="btn small" onclick={() => confirm(entry)}>Looks right</button
												>
												<button class="btn ghost small" onclick={() => startChoosing(entry)}>
													Change
												</button>
											{:else if openGroup === 'check' || openGroup === 'missing'}
												<button class="btn small" onclick={() => startChoosing(entry)}>Match</button
												>
											{:else if openGroup === 'ignored'}
												<button class="btn ghost small" onclick={() => startChoosing(entry)}>
													Match
												</button>
											{:else if openGroup !== 'later'}
												<button class="btn ghost small" onclick={() => startChoosing(entry)}>
													Change
												</button>
											{/if}
										</div>
									{/if}
								</div>
								{#if choosing === entry.media_id}
									{@render chooser(entry)}
								{/if}
							</li>
						{:else}
							<li class="faint small">Nothing by that name.</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	{/if}
</div>

<style>
	.anilist {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 15px 18px;
	}

	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px 16px;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.label .faint,
	.small {
		font-size: 0.82rem;
	}

	.small {
		margin: 0;
	}

	.buttons,
	.actions,
	.row-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.row-actions {
		flex: none;
		justify-content: flex-end;
	}

	.problem {
		margin: 0;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		font-size: 0.85rem;
	}

	.nudge {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: 0.85rem;
		color: var(--text-dim);
	}

	.nudge :global(svg) {
		color: var(--accent);
		flex: none;
	}

	.groups {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.group-chip {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px 6px 12px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text);
		font: inherit;
		font-size: 0.82rem;
		font-weight: 600;
		cursor: pointer;
		transition:
			border-color var(--fast) var(--ease),
			background var(--fast) var(--ease);
	}

	.group-chip:hover {
		border-color: var(--line-strong);
	}

	.group-chip.on {
		border-color: var(--accent);
	}

	.count {
		min-width: 22px;
		padding: 1px 7px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
		color: var(--text-dim);
		font-size: 0.74rem;
		text-align: center;
	}

	.group-chip.attention .count {
		background: var(--accent);
		color: var(--accent-ink);
	}

	.panel {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.filter {
		max-width: 320px;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		border-top: 1px solid var(--line);
	}

	.list > li {
		padding: 10px 0;
		border-bottom: 1px solid var(--line);
	}

	.entry {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}

	.entry > .label {
		flex: 1;
	}

	.thumb {
		width: 36px;
		height: 54px;
		border-radius: 4px;
		object-fit: cover;
		flex: none;
		background: var(--bg-elev-2);
	}

	.where,
	.how,
	.note,
	.doubt {
		font-size: 0.78rem;
	}

	.where {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--text);
	}

	.where :global(svg) {
		color: var(--text-faint);
		flex: none;
	}

	.how {
		color: var(--text-faint);
	}

	.note {
		color: var(--text-dim);
	}

	.doubt {
		color: var(--text-faint);
	}

	.chooser {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-top: 10px;
	}

	.find {
		display: flex;
		gap: 8px;
	}

	.find .input {
		flex: 1;
		min-width: 0;
	}

	.place {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
		align-items: flex-end;
	}

	.episode {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 0.78rem;
		color: var(--text-dim);
		width: 110px;
	}

	.picked {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.results {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.result {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 6px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev-2);
		text-align: left;
		cursor: pointer;
	}

	.result .label {
		flex: 1;
	}

	.result:hover {
		border-color: var(--accent);
	}

	.result img,
	.result .blank {
		width: 34px;
		height: 51px;
		border-radius: 4px;
		object-fit: cover;
		flex: none;
		background: var(--bg-elev);
	}

	.blank {
		display: grid;
		place-items: center;
		color: var(--text-faint);
	}

	.link {
		background: none;
		border: 0;
		padding: 0;
		color: var(--accent);
		cursor: pointer;
		font: inherit;
	}

	@media (max-width: 560px) {
		.entry {
			flex-wrap: wrap;
		}

		.row-actions.pair {
			width: 100%;
			padding-left: 48px;
			justify-content: flex-start;
		}
	}
</style>
