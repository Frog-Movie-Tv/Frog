<script lang="ts">
	import { api, img } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import { plural, relativeDate } from '#lib/format';
	import { toasts } from '#lib/toast.svelte';
	import type { AniListEntry, AniListStatus, Kind, SearchItem, SeasonSummary } from '#lib/types';

	let status = $state<AniListStatus | null>(null);
	let entries = $state<AniListEntry[]>([]);
	let busy = $state(false);
	let showAll = $state(false);
	let showUnsure = $state(false);
	/** The entry whose place is being chosen. */
	let choosing = $state<number | null>(null);
	let query = $state('');
	let results = $state<SearchItem[]>([]);
	let season = $state(1);
	let fromEpisode = $state(1);
	/** The show picked for an entry, waiting for its season to be confirmed. */
	let picked = $state<{ tmdb_id: number; name: string; seasons: SeasonSummary[] } | null>(null);
	let picking = $state(false);
	const pickedSeason = $derived(picked?.seasons.find((x) => x.season_number === season) ?? null);
	let leaving = $state(false);

	const unsure = $derived(entries.filter((e) => e.state === 'unsure'));
	const placed = $derived(entries.filter((e) => e.state === 'placed'));
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

	function startChoosing(entry: AniListEntry) {
		choosing = entry.media_id;
		picked = null;
		query = entry.name;
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
		if (result.kind === 'movie') {
			await choose(entry, 'movie', result.tmdb_id);
			return;
		}
		picking = true;
		try {
			const detail = await api.title('tv', result.tmdb_id);
			const seasons = detail.seasons.filter((x) => x.season_number >= 1);
			const spot = suggest(entry, result.tmdb_id, seasons);
			season = spot.season;
			fromEpisode = spot.episode;
			picked = { tmdb_id: result.tmdb_id, name: result.name, seasons };
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
			toasts.push(`${entry.name} is placed; it syncs in a moment`);
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
		return `${t.name} · season ${t.season ?? 1}${t.from_episode > 1 ? `, from episode ${t.from_episode}` : ''}`;
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
						label: x.name || `Season ${x.season_number}`,
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
					Place here
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
									<span class="blank"></span>
								{/if}
								<span class="label">
									<strong>{r.name}</strong>
									<span class="faint"
										>{r.kind === 'tv' ? 'Show' : 'Film'}{r.year ? ` · ${r.year}` : ''}</span
									>
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
				<button class="btn ghost small" onclick={() => ignore(entry)}>Not in Frog</button>
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
					Connected as {status.name} · synced {relativeDate(status.synced_at)}
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
		{#if status.error}
			<p class="problem">{status.error}</p>
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
			<p class="faint small">
				{plural(status.summary.entries, 'entry', 'entries')} on AniList · last sync sent {status
					.summary.to_anilist} to AniList and brought {status.summary.to_frog} into Frog
			</p>
		{/if}

		{#if unsure.length}
			<div class="group">
				<button class="link more" onclick={() => (showUnsure = !showUnsure)}>
					{showUnsure ? 'Hide unmatched' : `${plural(unsure.length, 'title')} not matched yet`}
				</button>
				{#if showUnsure}
					<p class="faint small">
						Frog looks again at every sync. Match one yourself if you would rather not wait.
					</p>
					<ul class="list">
						{#each unsure as entry (entry.media_id)}
							<li>
								<div class="entry">
									<span class="label">
										<strong>{entry.name}</strong>
										<span class="faint"
											>{entry.format ?? 'Anime'}{entry.episodes
												? ` · ${plural(entry.episodes, 'episode')}`
												: ''}</span
										>
									</span>
									{#if choosing !== entry.media_id}
										<button class="btn small" onclick={() => startChoosing(entry)}>Match</button>
									{/if}
								</div>
								{#if choosing === entry.media_id}
									{@render chooser(entry)}
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/if}

		{#if placed.length}
			<button class="link more" onclick={() => (showAll = !showAll)}>
				{showAll ? 'Hide matches' : `See all ${placed.length} matches`}
			</button>
			{#if showAll}
				<ul class="list">
					{#each placed as entry (entry.media_id)}
						<li>
							<div class="entry">
								<span class="label">
									<strong>{entry.name}</strong>
									<span class="faint">{where(entry)}</span>
								</span>
								{#if choosing !== entry.media_id}
									<button class="btn ghost small" onclick={() => startChoosing(entry)}>
										Change
									</button>
								{/if}
							</div>
							{#if choosing === entry.media_id}
								{@render chooser(entry)}
							{/if}
						</li>
					{/each}
				</ul>
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
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.problem {
		margin: 0;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		font-size: 0.85rem;
	}

	.list {
		list-style: none;
		margin: 8px 0 0;
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
		align-items: center;
		justify-content: space-between;
		gap: 12px;
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
	}

	.place {
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

	.picked .label {
		display: flex;
		flex-direction: column;
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

	.result:hover {
		border-color: var(--accent);
	}

	.result img,
	.blank {
		width: 34px;
		height: 51px;
		border-radius: 4px;
		object-fit: cover;
		flex: none;
		background: var(--bg-elev);
	}

	.link {
		background: none;
		border: 0;
		padding: 0;
		color: var(--accent);
		cursor: pointer;
		font: inherit;
	}

	.more {
		align-self: flex-start;
		font-size: 0.85rem;
	}
</style>
