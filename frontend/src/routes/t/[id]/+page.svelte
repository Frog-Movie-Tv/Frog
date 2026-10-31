<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import AddToShelf from '#lib/components/AddToShelf.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Poster from '#lib/components/Poster.svelte';
	import Rail from '#lib/components/Rail.svelte';
	import Scores from '#lib/components/Scores.svelte';
	import SeasonList from '#lib/components/SeasonList.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import TrackControls from '#lib/components/TrackControls.svelte';
	import { failure, idParam, type Failure } from '#lib/failure';
	import { episodeCode, prettyDate, runtime, year } from '#lib/format';
	import { providerLink } from '#lib/providers';
	import { settings } from '#lib/settings.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { titleKey } from '#lib/titles';
	import { uniqueBy } from '#lib/unique';
	import { whenWatched } from '#lib/when.svelte';
	import type { LibraryItem, TitleDetail, TitleScore, TitleScores } from '#lib/types';

	let detail = $state<TitleDetail | null>(null);
	let loading = $state(true);
	let error = $state<Failure | null>(null);
	let trailerOpen = $state<string | null>(null);
	const today = new Date().toISOString().slice(0, 10);
	let outside = $state<TitleScores | null>(null);

	/** Frog's id for the title: the page's own address. */
	const titleId = $derived(idParam(page.params.id));

	$effect(() => {
		void load(titleId);
	});

	let token = 0;

	const NOT_ON_TMDB = {
		title: "This title isn't on TMDB",
		hint: 'The link may be wrong, or TMDB has removed it.'
	};

	async function load(id: number | null) {
		const mine = ++token;
		if (id === null) {
			error = { missing: true, heading: 'Not found', ...NOT_ON_TMDB };
			detail = null;
			loading = false;
			return;
		}
		loading = true;
		error = null;
		try {
			const res = await api.title(id);
			if (mine !== token) return;
			// Known from AniList alone before, and on TMDB now: that is its page.
			if (res.moved_to) {
				await goto(`/t/${res.moved_to.id}`, { replaceState: true });
				return;
			}
			detail = res;
			outside = null;
			noteDraft = null;
			// Scores may ask another site; the page never waits for them.
			void api
				.scores(id)
				.then((r) => {
					if (mine === token) outside = r;
				})
				.catch(() => {
					// The title's own score is already on the page.
				});
		} catch (e) {
			if (mine !== token) return;
			error = failure(e, NOT_ON_TMDB);
			detail = null;
		} finally {
			if (mine === token) loading = false;
		}
	}

	function updateItem(item: LibraryItem) {
		if (detail) detail = { ...detail, item };
	}

	async function refreshAfterEpisodes(item: LibraryItem) {
		updateItem(item);
		// Pull "up next" and the progress again, cheaply, after a tick change.
		try {
			if (titleId === null) return;
			const fresh = await api.title(titleId);
			detail = fresh;
		} catch {
			// The optimistic update already landed; the rest can wait.
		}
	}

	const item = $derived(detail?.item);
	const accent = $derived(settings.current.followPoster && item ? item.accent : null);

	/** Text on the poster's colour: dark on a light one, white on a dark one. */
	function inkFor(hex: string): string {
		const n = parseInt(hex.replace('#', '').padEnd(6, '0').slice(0, 6), 16);
		const lin = (c: number) => {
			const v = c / 255;
			return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
		};
		const l = 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
		return l > 0.36 ? '#0b1410' : '#ffffff';
	}

	/** What is being typed; null shows the saved note. */
	let noteDraft = $state<string | null>(null);
	let savingNote = $state(false);
	const savedNote = $derived(item?.entry?.notes ?? '');
	const noteText = $derived(noteDraft ?? savedNote);

	async function saveNote() {
		if (!item) return;
		const id = item.id;
		const before = savedNote;
		const text = noteText.trim();
		savingNote = true;
		try {
			const { item: updated } = await api.track(id, { notes: text || null });
			updateItem(updated);
			noteDraft = null;
			if (!text && before) {
				toasts.undoable('Note removed', async () => {
					const { item: back } = await api.track(id, { notes: before });
					updateItem(back);
				});
			} else {
				toasts.push('Note saved');
			}
		} catch (e) {
			toasts.error(e);
		} finally {
			savingNote = false;
		}
	}

	/** Its own provider's score straight away, the rest as they arrive. */
	const scores = $derived.by<TitleScore[]>(() => {
		if (outside?.ratings.length) return outside.ratings;
		if (!item?.tmdb_vote) return [];
		const source = detail?.anilist_only ? 'anilist' : 'tmdb';
		return [{ source, score: item.tmdb_vote * 10, votes: null }];
	});

	const metaLine = $derived.by(() => {
		if (!item) return [];
		const bits: string[] = [];
		const y = year(item.release_date);
		if (y) bits.push(y);
		const age = detail?.age_rating ?? outside?.age_rating;
		if (age) bits.push(`Rated ${age}`);
		if (item.kind === 'movie' && item.runtime) bits.push(runtime(item.runtime));
		if (item.kind === 'tv' && item.season_count) {
			bits.push(`${item.season_count} season${item.season_count === 1 ? '' : 's'}`);
		}
		if (item.kind === 'tv' && item.episode_count) bits.push(`${item.episode_count} episodes`);
		if (item.tmdb_status && item.kind === 'tv') bits.push(item.tmdb_status);
		if (detail?.anilist_only) bits.push('From AniList');
		return [...new Set(bits)];
	});

	/** One card per person: a writer-director reads as one name, not two. */
	const crew = $derived.by(() => {
		const people: { id: number; name: string; jobs: string[] }[] = [];
		for (const c of detail?.crew ?? []) {
			const known = people.find((p) => p.id === c.id);
			if (known) {
				if (!known.jobs.includes(c.job)) known.jobs.push(c.job);
			} else {
				people.push({ id: c.id, name: c.name, jobs: [c.job] });
			}
		}
		return people;
	});

	const cast = $derived(uniqueBy(detail?.cast ?? [], (p) => p.id));
	const trailers = $derived(uniqueBy(detail?.trailers ?? [], (t) => t.key));
	const parts = $derived(uniqueBy(detail?.collection?.parts ?? [], titleKey));
	const recommendations = $derived(uniqueBy(detail?.recommendations ?? [], titleKey));

	const playing = $derived(trailers.find((t) => t.key === trailerOpen) ?? null);

	const providers = $derived(detail?.providers);
	const providerGroups = $derived(
		providers
			? [
					{ label: 'Stream', list: uniqueBy(providers.flatrate, (p) => p.id) },
					{ label: 'Rent', list: uniqueBy(providers.rent, (p) => p.id) },
					{ label: 'Buy', list: uniqueBy(providers.buy, (p) => p.id) }
				].filter((g) => g.list.length)
			: []
	);
	const hasProviders = $derived(providerGroups.length > 0);

	let markingAll = $state(false);
	/** Bumped to remount the season list after a whole-show change. */
	let seasonsVersion = $state(0);

	async function markAllWatched() {
		if (!item) return;
		const answer = await whenWatched.ask(item.name, 'Every aired episode');
		if (!answer) return;
		markingAll = true;
		try {
			const { item: updated } = await api.setShowWatched(item.id, true, answer);
			await refreshAfterEpisodes(updated);
			seasonsVersion++;
			toasts.push(`Marked ${updated.watched_episodes} episodes watched`);
		} catch (e) {
			toasts.error(e);
		} finally {
			markingAll = false;
		}
	}

	async function toggleNotify() {
		if (!item?.entry) return;
		const notify = !item.entry.notify;
		try {
			const { item: updated } = await api.track(item.id, { notify });
			updateItem(updated);
			toasts.push(notify ? 'Alerts on for new episodes' : 'Alerts off for this show');
		} catch (e) {
			toasts.error(e);
		}
	}

	async function markFilmWatched() {
		if (!item) return;
		try {
			const { item: updated } = await api.track(item.id, { status: 'completed' });
			updateItem(updated);
			toasts.push('Marked as watched');
		} catch (e) {
			toasts.error(e);
		}
	}
</script>

<svelte:head>
	<title>{item ? `${item.name} · Frog` : error ? `${error.heading} · Frog` : 'Frog'}</title>
</svelte:head>

{#if loading && !detail}
	<div class="page"><Spinner label="Loading" /></div>
{:else if error}
	<div class="page">
		<Empty art={error.missing ? 'search' : 'rest'} title={error.title} hint={error.hint}>
			{#if !error.missing}
				<button class="btn primary" onclick={() => load(titleId)}>Try again</button>
			{/if}
			<a class="btn" class:primary={error.missing} href="/">Go home</a>
		</Empty>
	</div>
{:else if detail && item}
	<article
		class="title-page"
		class:tinted={!!accent}
		style:--poster-accent={accent?.accent}
		style:--poster-ink={accent ? inkFor(accent.accent) : undefined}
		style:--accent-soft={accent
			? `color-mix(in srgb, ${accent.accent} 18%, transparent)`
			: undefined}
		style:--page-shade={accent?.shade ?? 'var(--bg)'}
	>
		<header class="hero">
			{#if item.backdrop_path}
				<img class="backdrop" src={img(item.backdrop_path, 'w1280')} alt="" fetchpriority="high" />
			{/if}
			<div class="veil" aria-hidden="true"></div>

			<div class="hero-inner">
				<div class="poster-slot" style:view-transition-name="poster-t{item.id}">
					{#if item.poster_path}
						<img src={img(item.poster_path, 'w342')} alt="Poster for {item.name}" />
					{/if}
				</div>

				<div class="hero-text">
					{#if item.logo_path}
						<img class="logo" src={img(item.logo_path, 'w500')} alt={item.name} />
					{:else}
						<h1>{item.name}</h1>
					{/if}

					{#if detail.tagline}
						<p class="tagline">{detail.tagline}</p>
					{/if}

					<div class="meta">
						{#each metaLine as bit, i (bit)}
							{#if i > 0}<span class="dot" aria-hidden="true">·</span>{/if}
							<span>{bit}</span>
						{/each}
					</div>
					{#if item.genres.length}
						<p class="genres">{item.genres.slice(0, 3).join(', ')}</p>
					{/if}

					<Scores {scores} />

					<div class="actions">
						<TrackControls
							{item}
							onchange={updateItem}
							onepisodes={async (updated) => {
								await refreshAfterEpisodes(updated);
								seasonsVersion++;
							}}
						/>
						{#if item.kind === 'movie' && item.entry && item.entry.status !== 'completed'}
							<button class="btn" onclick={markFilmWatched}>Mark watched</button>
						{/if}
						{#if item.kind === 'tv' && item.entry && ['watching', 'planned', 'paused'].includes(item.entry.status)}
							{@const on = item.entry.notify}
							<button
								class="btn bell"
								class:on
								aria-pressed={on}
								aria-label={on ? 'New episode alerts on' : 'New episode alerts off'}
								title={on ? 'New episode alerts on' : 'New episode alerts off'}
								onclick={toggleNotify}
							>
								<Icon name={on ? 'bell' : 'bell-off'} size={17} />
							</button>
						{/if}
						<AddToShelf titleId={item.id} name={item.name} />
					</div>

					{#if item.total_episodes > 0 && item.entry}
						<a class="progress-line hit" href="#seasons">
							<span class="bar">
								<i style:width="{(item.watched_episodes / item.total_episodes) * 100}%"></i>
							</span>
							<span class="count">{item.watched_episodes} of {item.total_episodes}</span>
							{#if detail.next_up}
								<span class="next">
									Up next
									<strong>
										{episodeCode(detail.next_up.season_number, detail.next_up.episode_number)}
									</strong>
									{#if detail.next_up.air_date && detail.next_up.air_date > today}
										· {prettyDate(detail.next_up.air_date)}
									{/if}
								</span>
							{/if}
						</a>
					{/if}
				</div>
			</div>
		</header>

		<div class="page body">
			<div class="layout">
				<div class="main">
					{#if item.overview}
						<section class="block">
							<p class="overview">{item.overview}</p>
						</section>
					{/if}

					{#if item.entry}
						<section class="block notes">
							<div class="section-head"><h2>Your notes</h2></div>
							<textarea
								class="input"
								rows="3"
								maxlength="2000"
								aria-label="Your notes on {item.name}"
								placeholder="What stayed with you, who you watched it with, where you left off."
								value={noteText}
								oninput={(e) => (noteDraft = e.currentTarget.value)}></textarea>
							{#if noteText.trim() !== savedNote}
								<div class="note-actions">
									<button class="btn ghost" onclick={() => (noteDraft = null)}>Cancel</button>
									<button class="btn primary" disabled={savingNote} onclick={saveNote}>
										{noteText.trim() ? 'Save note' : 'Remove note'}
									</button>
								</div>
							{/if}
						</section>
					{/if}
				</div>

				<aside class="side">
					{#if hasProviders && providers}
						<section class="block">
							<div class="section-head">
								<h2>Where to watch</h2>
								{#if !detail.anilist_only}<span class="faint">{detail.provider_region}</span>{/if}
							</div>
							<div class="providers">
								{#each providerGroups as group (group.label)}
									{@const label = group.label}
									{@const list = group.list}
									{#if list.length}
										<div class="pgroup">
											<span class="eyebrow">{label}</span>
											<div class="logos">
												{#each list as p (p.id)}
													{@const href =
														p.url ?? providerLink(p.name, item.name, providers?.link ?? null)}
													<a
														class="plogo"
														{href}
														target="_blank"
														rel="noreferrer noopener"
														title="Find {item.name} on {p.name}"
													>
														{#if p.logo_path}
															<img src={img(p.logo_path, 'w92')} alt={p.name} loading="lazy" />
														{:else}
															{p.name}
														{/if}
													</a>
												{/each}
											</div>
										</div>
									{/if}
								{/each}
							</div>
						</section>
					{/if}

					{#if crew.length}
						<section class="block crew">
							<div class="section-head"><h2>Crew</h2></div>
							<div class="crew-list">
								{#each crew as c (c.id)}
									<a href="/person/{c.id}" class="crew-item">
										<strong>{c.name}</strong>
										<span class="faint">{c.jobs.join(', ')}</span>
									</a>
								{/each}
							</div>
						</section>
					{/if}
				</aside>
			</div>

			{#if item.kind === 'tv' && detail.seasons.length}
				<section class="block" id="seasons">
					<div class="section-head">
						<h2>Episodes</h2>
						<button class="btn ghost small" disabled={markingAll} onclick={markAllWatched}>
							<Icon name="check" size={14} />
							Mark all watched
						</button>
					</div>
					{#key seasonsVersion}
						<SeasonList
							titleId={item.id}
							showName={item.name}
							fallbackStill={item.backdrop_path}
							seasons={detail.seasons}
							ratings={detail.ratings ?? []}
							initialSeason={detail.next_up?.season_number ?? null}
							onprogress={refreshAfterEpisodes}
						/>
					{/key}
				</section>
			{/if}

			{#if cast.length}
				<section class="block">
					<Rail title="Cast" width="108px">
						{#each cast as person (person.id)}
							<a class="person" href="/person/{person.id}">
								<div class="face">
									{#if person.profile_path}
										<img src={img(person.profile_path, 'w185')} alt="" loading="lazy" />
									{:else}
										<span aria-hidden="true">{person.name.slice(0, 1)}</span>
									{/if}
								</div>
								<span class="pname">{person.name}</span>
								{#if person.character}<span class="role faint">{person.character}</span>{/if}
							</a>
						{/each}
					</Rail>
				</section>
			{/if}

			{#if trailers.length}
				<section class="block">
					{#if playing}
						<div class="player">
							<!-- YouTube refuses to play (error 153) without knowing which site
							     embeds it, and the site sends no referrer anywhere else, so this
							     one frame gets the address of Frog and nothing more. -->
							<iframe
								src="https://www.youtube-nocookie.com/embed/{playing.key}?autoplay=1"
								title={playing.name}
								referrerpolicy="strict-origin-when-cross-origin"
								allow="accelerometer; autoplay; clipboard-write; encrypted-media; picture-in-picture"
								allowfullscreen
							></iframe>
						</div>
					{/if}
					<Rail title={trailers.length === 1 ? 'Trailer' : 'Trailers'} wide>
						{#each trailers as t (t.key)}
							<button
								class="trailer"
								class:on={trailerOpen === t.key}
								aria-pressed={trailerOpen === t.key}
								onclick={() => (trailerOpen = t.key)}
							>
								{#if item.backdrop_path}
									<img class="still" src={img(item.backdrop_path, 'w780')} alt="" loading="lazy" />
								{/if}
								<span class="sheen" aria-hidden="true"></span>
								<span class="play" aria-hidden="true">
									<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
										<path
											d="M8 5.14v13.72a1 1 0 0 0 1.5.86l11-6.86a1 1 0 0 0 0-1.72l-11-6.86A1 1 0 0 0 8 5.14z"
										/>
									</svg>
								</span>
								<span class="tname">{t.name}</span>
								<span class="tkind">{t.kind}</span>
							</button>
						{/each}
					</Rail>
					<p class="faint tiny trailer-note">Playing a trailer loads it from YouTube.</p>
				</section>
			{/if}

			{#if parts.length}
				<section class="block">
					<Rail title={detail.collection?.name ?? 'Franchise'}>
						{#each parts as part (titleKey(part))}
							<Poster
								kind={part.kind}
								ref={part}
								name={part.name}
								posterPath={part.poster_path}
								year={part.year}
								status={part.in_library}
							/>
						{/each}
					</Rail>
					<p class="faint tiny">Franchise shown in release order.</p>
				</section>
			{/if}

			{#if recommendations.length}
				<section class="block">
					<Rail title="More like this">
						{#each recommendations as rec (titleKey(rec))}
							<Poster
								kind={rec.kind}
								ref={rec}
								name={rec.name}
								posterPath={rec.poster_path}
								year={rec.year}
								status={rec.in_library}
							/>
						{/each}
					</Rail>
				</section>
			{/if}
		</div>
	</article>
{/if}

<style>
	.notes textarea {
		display: block;
		max-width: 76ch;
		resize: vertical;
		line-height: 1.55;
	}

	.note-actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
		max-width: 76ch;
		margin-top: 10px;
	}

	.title-page.tinted {
		--accent: var(--poster-accent);
		--accent-ink: var(--poster-ink);
	}

	/* On a light page a pale poster colour would vanish as text and icons:
	   darken it, and put white on it. */
	:global(:root[data-theme='light']) .title-page.tinted,
	:global(:root[data-theme='pond-light']) .title-page.tinted {
		--accent: color-mix(in oklab, var(--poster-accent) 58%, #000);
		--accent-ink: #fff;
	}

	.title-page {
		/* The poster's dark companion sits behind the artwork, which is what
		   makes two different films feel like two different pages. It stays
		   inside the hero, which fades to the page, so nothing shows a seam. */
		background: var(--bg);
		min-height: 100dvh;
	}

	.hero {
		position: relative;
		isolation: isolate;
		background: linear-gradient(var(--page-shade) 50%, var(--bg));
		padding: calc(var(--header-h) + 56px) var(--page-pad) 32px;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 18%;
		z-index: -2;
		/* The picture fades out itself, so its edge never shows as a line where
		   the page begins. */
		mask-image: linear-gradient(to top, transparent, #000 30%);
	}

	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(
				to top,
				var(--bg) 2%,
				color-mix(in srgb, var(--bg) 55%, transparent) 45%,
				transparent
			),
			linear-gradient(
				to right,
				color-mix(in srgb, var(--page-shade) 85%, transparent),
				transparent 70%
			);
	}

	.hero-inner {
		max-width: 1440px;
		margin: 0 auto;
		display: flex;
		gap: 36px;
		align-items: flex-end;
		min-height: 360px;
	}

	.poster-slot {
		flex: none;
		width: 210px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius);
		overflow: hidden;
		box-shadow: var(--shadow-3);
		background: var(--bg-elev-2);
	}

	.poster-slot img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.hero-text {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
		padding-bottom: 4px;
	}

	.logo {
		max-width: min(420px, 70vw);
		max-height: 120px;
		margin-bottom: 6px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.6));
	}

	/* On a dark theme a black logo would vanish into the page; a faint light
	   halo keeps its outline without changing a coloured one. */
	:global(:root:not([data-theme='light']):not([data-theme='pond-light'])) .logo {
		filter: drop-shadow(0 0 1px rgb(255 255 255 / 0.45))
			drop-shadow(0 0 14px rgb(255 255 255 / 0.12)) drop-shadow(0 4px 18px rgb(0 0 0 / 0.5));
	}

	.tagline {
		font-style: italic;
		color: var(--text-dim);
		font-size: 0.95rem;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 8px;
		font-size: 0.9rem;
		color: var(--text);
	}

	.dot {
		color: var(--text-faint);
	}

	.genres {
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	/* Every control here is one family: the same height, the same pill, the
	   same surface over the artwork. Only "Add to library" stands out. */
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 8px;
	}

	.actions :global(.btn),
	.actions :global(.trigger),
	.actions :global(.rating > .chip) {
		height: 42px;
		min-height: 42px;
		padding: 0 16px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--bg-elev) 86%, transparent);
		color: var(--text);
		font-size: 0.88rem;
		font-weight: 600;
	}

	.actions :global(.btn:hover),
	.actions :global(.trigger:hover),
	.actions :global(.rating > .chip:hover) {
		border-color: var(--line-strong);
		background: var(--bg-elev);
	}

	.actions :global(.btn.primary) {
		background: var(--accent);
		border-color: transparent;
		color: var(--accent-ink);
	}

	.actions :global(.btn.icon),
	.actions :global(.bell) {
		width: 42px;
		padding: 0;
	}

	.actions :global(.rating > .chip.rated) {
		color: var(--text);
	}

	.actions :global(.rating > .chip.rated svg) {
		color: var(--accent);
	}

	/* The status menu leads the row, a touch wider than the rest. */
	.actions :global(.select) {
		width: auto;
		min-width: 140px;
	}

	.progress-line {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 12px;
		margin-top: 6px;
		font-size: 0.84rem;
		color: var(--text-dim);
	}

	.progress-line:hover .next strong {
		color: var(--accent);
	}

	.count {
		font-variant-numeric: tabular-nums;
	}

	.next strong {
		margin-left: 4px;
		color: var(--text);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.bar {
		display: block;
		width: min(220px, 40vw);
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width var(--slow) var(--ease);
	}

	.body {
		padding-top: 32px;
	}

	/* What you came for on the left; where to watch and who made it beside
	   it, at hand without pushing the episodes down. */
	.layout {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 300px;
		gap: 48px;
		align-items: start;
	}

	.main {
		min-width: 0;
	}

	.side .block {
		margin-bottom: 32px;
	}

	.side .section-head h2 {
		font-size: 1rem;
	}

	.side .crew-list {
		grid-template-columns: 1fr;
	}

	@media (max-width: 1100px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			gap: 0;
		}
	}

	.block {
		margin-bottom: 44px;
	}

	.btn.small {
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.overview {
		max-width: 72ch;
		font-size: 1.05rem;
		line-height: 1.65;
		color: var(--text-dim);
	}

	.providers {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 22px;
	}

	.pgroup {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.logos {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	.plogo {
		display: inline-flex;
		border-radius: 11px;
		transition:
			transform var(--fast) var(--ease),
			box-shadow var(--fast) var(--ease);
	}

	.plogo:hover {
		transform: translateY(-2px);
		box-shadow: var(--shadow-2);
	}

	.plogo img {
		width: 44px;
		height: 44px;
		border-radius: 11px;
		object-fit: cover;
		border: 1px solid var(--line);
	}

	.person {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	/* The whole photograph: portraits are 2:3, and a circle cut through faces. */
	.face {
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		display: grid;
		place-items: center;
		font-size: 1.3rem;
		color: var(--text-faint);
		transition: transform var(--slow) var(--ease);
	}

	.person:hover .face {
		transform: scale(1.04);
	}

	.face img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}

	.pname {
		font-size: 0.82rem;
		font-weight: 550;
		line-height: 1.25;
	}

	.role {
		font-size: 0.73rem;
		line-height: 1.25;
	}

	.crew-list {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 0 16px;
	}

	.crew-item {
		display: flex;
		flex-direction: column;
		padding: 9px 0;
		border-bottom: 1px solid var(--line);
		font-size: 0.88rem;
	}

	.crew-item:hover strong {
		color: var(--accent);
	}

	.crew-item span {
		font-size: 0.75rem;
	}

	.trailer {
		position: relative;
		display: block;
		width: 100%;
		padding: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
		cursor: pointer;
		aspect-ratio: 16 / 9;
		transition: border-color var(--fast) var(--ease);
	}

	.trailer.on {
		border-color: var(--accent);
	}

	/* The title's own backdrop, never YouTube's thumbnail: nothing should
	   reach a third party before you press play, and a removed video used to
	   404 in the console. */
	.still {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		transition: transform var(--slow) var(--ease);
	}

	.trailer:hover .still {
		transform: scale(1.04);
	}

	.sheen {
		position: absolute;
		inset: 0;
		background:
			linear-gradient(
				180deg,
				color-mix(in srgb, var(--bg) 55%, transparent),
				transparent 35%,
				transparent 55%,
				color-mix(in srgb, var(--bg) 88%, transparent)
			),
			radial-gradient(
				120% 100% at 50% 0%,
				color-mix(in srgb, var(--accent) 14%, transparent),
				transparent 70%
			);
	}

	.play {
		position: absolute;
		top: 50%;
		left: 50%;
		display: grid;
		place-items: center;
		width: 46px;
		height: 46px;
		margin: -23px 0 0 -23px;
		padding-left: 3px;
		border-radius: 50%;
		color: #fff;
		background: rgba(0, 0, 0, 0.45);
		backdrop-filter: blur(6px);
		border: 1px solid rgba(255, 255, 255, 0.25);
		transition:
			transform var(--fast) var(--ease),
			background var(--fast) var(--ease);
	}

	.trailer:hover .play,
	.trailer.on .play {
		transform: scale(1.1);
		background: var(--accent);
		color: var(--accent-ink);
		border-color: transparent;
	}

	.tkind {
		position: absolute;
		top: 9px;
		left: 11px;
		font-size: 0.66rem;
		font-weight: 650;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-dim);
	}

	.tname {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		padding: 0 12px 10px;
		font-size: 0.82rem;
		font-weight: 600;
		text-align: left;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.trailer-note {
		margin-top: -26px;
	}

	.player {
		aspect-ratio: 16 / 9;
		max-width: 960px;
		margin-bottom: 18px;
		border-radius: var(--radius-sm);
		overflow: hidden;
		border: 1px solid var(--line-strong);
		background: #000;
	}

	.player iframe {
		width: 100%;
		height: 100%;
		border: none;
	}

	.tiny {
		font-size: 0.72rem;
		margin-top: 10px;
	}

	@media (max-width: 860px) {
		.hero-inner {
			min-height: 0;
			gap: 24px;
		}

		.poster-slot {
			width: 150px;
		}

		.hero {
			padding-top: calc(var(--header-h) + 120px);
		}
	}

	/* A phone: the artwork and the logo already say what this is, so the
	   poster steps aside and the shelf button keeps only its icon. */
	@media (max-width: 600px) {
		.poster-slot,
		.tagline {
			display: none;
		}

		.hero {
			padding-top: calc(var(--header-h) + 150px);
		}

		.logo {
			max-height: 90px;
		}

		.actions :global(.btn .label) {
			display: none;
		}

		.actions :global(.btn:has(.label)) {
			width: 42px;
			padding: 0;
		}
	}

	.bell.on {
		color: var(--accent);
	}
</style>
