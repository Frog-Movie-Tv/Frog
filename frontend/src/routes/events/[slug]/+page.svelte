<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { addFrom } from '#lib/add';
	import { address } from '#lib/address';
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Countdown from '#lib/components/Countdown.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import Premiere from '#lib/components/Premiere.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import {
		afterLine,
		eventDay,
		isOut,
		itemFacts,
		itemState,
		label,
		readyLine,
		shouldPlay
	} from '#lib/events';
	import { failure, type Failure } from '#lib/failure';
	import { runtime } from '#lib/format';
	import { showFrom } from '#lib/menu.svelte';
	import { personName } from '#lib/people.svelte';
	import { session } from '#lib/session.svelte';
	import { titleHref } from '#lib/titles';
	import { toasts } from '#lib/toast.svelte';
	import type { EventListItem, EventPage, TitleDetail } from '#lib/types';
	import { statusLabel } from '#lib/types';

	const slug = $derived(page.params.slug ?? '');

	let e = $state<EventPage | null>(null);
	let failed = $state<Failure | null>(null);
	let detail = $state<TitleDetail | null>(null);

	async function load() {
		failed = null;
		try {
			e = await api.event(slug);
		} catch (err) {
			failed = failure(err, { title: 'Could not load this event', hint: 'Try again in a minute.' });
		}
	}

	$effect(() => {
		if (slug) void load();
	});

	// Without a catch-up list the page is about the title itself.
	$effect(() => {
		if (!e || e.catch_up || detail?.item.id === e.title.id) return;
		void api
			.title(e.title.id)
			.then((d) => (detail = d))
			.catch(() => undefined);
	});

	let now = $state(Date.now());
	$effect(() => {
		const every = setInterval(() => (now = Date.now()), 15_000);
		return () => clearInterval(every);
	});
	const out = $derived(!!e && isOut(e, now));
	const r = $derived(e?.readiness ?? null);
	const line = $derived(r ? (out ? afterLine(r) : readyLine(r)) : null);
	const past = $derived(e?.phase === 'past');

	// --- the premiere, on the first open after zero -------------------------
	let premiere = $state(false);
	let preview = $state(false);
	let head = $state<HTMLElement | null>(null);
	let tried = false;
	$effect(() => {
		if (!e || tried) return;
		tried = true;
		if (shouldPlay(e, Date.now())) premiere = true;
	});

	// --- filters, in the address --------------------------------------------
	const SHOWS = ['all', 'essentials', 'left'] as const;
	type Show = (typeof SHOWS)[number];
	const show = $derived.by((): Show => {
		const asked = address().searchParams.get('show');
		return SHOWS.find((s) => s === asked) ?? 'all';
	});

	function choose(next: Show) {
		// The list's filter is the only thing this page keeps in its address.
		const here = address();
		void goto(next === 'all' ? here.pathname : `${here.pathname}?show=${next}`, {
			shallow: true,
			replace: true,
			state: page.state
		});
	}

	const keep = (li: EventListItem) =>
		show === 'all' || (show === 'essentials' ? li.essential : !li.standing.seen);

	/** Numbered in watch order across the groups, whatever is filtered out. */
	const groups = $derived.by(() => {
		if (!e) return [];
		let n = 0;
		return e.groups
			.map((g) => ({
				name: g.name,
				titles: g.titles.map((li) => ({ li, n: ++n })).filter(({ li }) => keep(li))
			}))
			.filter((g) => g.titles.length);
	});

	// --- acting ----------------------------------------------------------------
	let busy = $state(false);

	async function hide() {
		if (!e) return;
		const s = e.slug;
		try {
			await api.hideEvent(s);
			e.hidden = true;
			toasts.undoable('Hidden from Home. It stays on Events, without reminders.', async () => {
				await api.showEvent(s);
				await load();
			});
		} catch (err) {
			toasts.error(err);
		}
	}

	async function unhide() {
		if (!e) return;
		try {
			await api.showEvent(e.slug);
			e.hidden = false;
		} catch (err) {
			toasts.error(err);
		}
	}

	async function add(li: EventListItem, ev: MouseEvent) {
		busy = true;
		try {
			await addFrom(li.item, ev.currentTarget as HTMLElement, ev, load);
		} finally {
			busy = false;
		}
	}

	function more(ev: MouseEvent) {
		if (!e) return;
		const here = e;
		showFrom(
			ev.currentTarget as HTMLElement,
			{
				title: here.title.name,
				items: () => [
					{
						label: here.title.kind === 'movie' ? 'Open the film' : 'Open the show',
						icon: here.title.kind === 'movie' ? 'film' : 'tv',
						run: () => void goto(`/t/${here.title.id}`)
					},
					...(session.status?.is_admin
						? [
								{
									label: 'Preview premiere',
									icon: 'play' as const,
									run: () => {
										preview = true;
										premiere = true;
									}
								}
							]
						: [])
				]
			},
			ev
		);
	}

	const figure = (a: number, b: number) => `${a} of ${b}`;
	const backdrop = $derived(e?.title.backdrop_path ?? null);
	const accent = $derived(e?.accent.accent ?? 'var(--accent)');
	const trailer = $derived(detail?.trailers.find((t) => t.site === 'YouTube') ?? null);
</script>

<svelte:head>
	<title>{e ? e.title.name : 'Event'} · Frog</title>
</svelte:head>

{#if failed}
	<div class="page"><LoadFailed {failed} onretry={load} /></div>
{:else if !e}
	<div class="page"><Spinner label="Loading event" /></div>
{:else}
	<div class="event" style:--event-accent={accent}>
		<header class="head">
			<div class="backdrop">
				{#if backdrop}
					<img src={img(backdrop, 'w1280')} alt="" />
				{/if}
			</div>
			<div class="veil"></div>
			<div class="head-body" class:waiting={premiere && !preview} bind:this={head}>
				<span class="label">
					{past ? `Premiered ${eventDay(e.day)}` : label(e, out)}
				</span>
				<a class="name-link" href={titleHref(e.title)}>
					{#if e.title.logo_path}
						<img class="logo" src={img(e.title.logo_path, 'w500')} alt={e.title.name} />
					{:else}
						<h1>{e.title.name}</h1>
					{/if}
				</a>
				{#if !out}
					<Countdown zero={e.zero} compact onzero={() => (now = Date.now())} />
				{/if}
				{#if line && !past}
					<p class="ready"><strong>{line.strong}</strong> {line.rest}</p>
				{/if}
				<div class="actions">
					{#if r && r.next && !past}
						<a class="btn primary" href="/t/{r.next.title_id}">
							<Icon name="play" size={14} filled />
							Catch up · {r.next.name}{r.next.code ? ` ${r.next.code}` : ''}
						</a>
					{/if}
					{#if !past}
						{#if e.hidden}
							<button class="btn on-picture" onclick={unhide}>
								<Icon name="eye" size={16} />
								Show on Home
							</button>
						{:else}
							<button class="btn on-picture" onclick={hide}>
								<Icon name="eye-off" size={16} />
								Hide from Home
							</button>
						{/if}
					{/if}
					<button class="btn on-picture more" aria-label="More" aria-haspopup="menu" onclick={more}>
						<Icon name="more" size={18} />
					</button>
				</div>
				{#if e.on === false}
					<p class="off">Switched off for everyone. Only administrators see it.</p>
				{/if}
			</div>
		</header>

		<div class="page body">
			{#if e.catch_up && r}
				<div class="figures">
					<div class="figure">
						<span>Essentials</span>
						<strong>{figure(r.essentials_seen, r.essentials)}</strong>
					</div>
					{#if r.optional}
						<div class="figure">
							<span>Optional</span>
							<strong>{figure(r.optional_seen, r.optional)}</strong>
						</div>
					{/if}
					<div class="figure">
						<span>Left to watch</span>
						<strong>{runtime(r.minutes_left) || 'Nothing'}</strong>
					</div>
					{#if !out && r.per_day}
						<div class="figure">
							<span>A day to make it</span>
							<strong>{runtime(r.per_day)}</strong>
						</div>
					{/if}
				</div>

				<div class="section-head list-head">
					<h2>In watch order</h2>
					<div class="segmented" role="group" aria-label="Show">
						<button
							class:on={show === 'all'}
							aria-pressed={show === 'all'}
							onclick={() => choose('all')}>Everything</button
						>
						<button
							class:on={show === 'essentials'}
							aria-pressed={show === 'essentials'}
							onclick={() => choose('essentials')}>Essentials</button
						>
						<button
							class:on={show === 'left'}
							aria-pressed={show === 'left'}
							onclick={() => choose('left')}>Still to watch</button
						>
					</div>
				</div>

				{#if !groups.length}
					<p class="all-seen">
						<Icon name="check" size={16} /> Every one of these is seen.
					</p>
				{/if}
				{#each groups as g (g.name)}
					<section class="group">
						<h3>{g.name}</h3>
						<ol>
							{#each g.titles as { li, n } (`${n}:${li.item.id}`)}
								{@const state = itemState(li)}
								<li class:seen={state === 'seen'}>
									<span class="n">{n}</span>
									<a class="poster" href={titleHref(li.item)} tabindex="-1" aria-hidden="true">
										{#if li.item.poster_path}
											<img src={img(li.item.poster_path, 'w154')} alt="" loading="lazy" />
										{/if}
									</a>
									<span class="words">
										<a class="name" href={titleHref(li.item)}>{li.item.name}</a>
										<span class="facts">{itemFacts(li)}</span>
										{#if li.note}<span class="note">{li.note}</span>{/if}
									</span>
									<span class="pill kind" class:optional={!li.essential}
										>{li.essential ? 'Essential' : 'Optional'}</span
									>
									<span class="state">
										{#if state === 'seen'}
											<span class="seen-mark"><Icon name="check" size={14} /> Seen</span>
										{:else if state === 'progress'}
											<span
												class="progress"
												aria-label="{li.standing.watched} of {li.standing.total}"
											>
												<span
													style:width="{Math.round(
														(li.standing.watched / Math.max(1, li.standing.total)) * 100
													)}%"
												></span>
											</span>
											<span class="small">{li.standing.watched} of {li.standing.total}</span>
										{:else if state === 'library'}
											<span class="small">In library</span>
										{:else}
											<button
												class="btn add"
												disabled={busy}
												aria-haspopup="menu"
												aria-label="Add {li.item.name}"
												onclick={(ev) => add(li, ev)}
											>
												<Icon name="plus" size={15} />
												Add
											</button>
										{/if}
									</span>
								</li>
							{/each}
						</ol>
					</section>
				{/each}
			{:else if detail}
				{#if detail.item.overview}
					<section class="about">
						<h2>About</h2>
						<p>{detail.item.overview}</p>
						{#if trailer}
							<a
								class="btn"
								href="https://www.youtube.com/watch?v={trailer.key}"
								target="_blank"
								rel="noopener noreferrer"
							>
								<Icon name="play" size={14} filled /> Trailer
							</a>
						{/if}
					</section>
				{/if}
				{#if detail.cast.length}
					<section class="cast">
						<h2>Cast and crew</h2>
						<ul>
							{#each detail.cast.slice(0, 12) as c (c.id)}
								<li>
									<a href="/person/{c.id}">
										<span class="face">
											{#if c.profile_path}<img
													src={img(c.profile_path, 'w185')}
													alt=""
													loading="lazy"
												/>{/if}
										</span>
										<strong>{c.name}</strong>
										{#if c.character}<span class="small">{c.character}</span>{/if}
									</a>
								</li>
							{/each}
						</ul>
					</section>
				{/if}
			{/if}

			{#if e.people.length}
				<section class="people">
					<h2>{e.catch_up ? "Who's ready" : "On everyone's watchlist"}</h2>
					<ul>
						{#each e.people as p (p.who.username)}
							<li>
								<Avatar src={p.who.avatar} name={personName(p.who)} size={34} />
								<span class="who">{p.me ? 'You' : personName(p.who)}</span>
								{#if e.catch_up}
									{#if p.ready}
										<span class="pill ready-chip">Ready</span>
									{:else}
										<span class="bar">
											<span
												style:width="{Math.round(
													((p.essentials_seen ?? 0) / Math.max(1, p.essentials ?? 1)) * 100
												)}%"
											></span>
										</span>
										<span class="small">{p.essentials_seen} of {p.essentials}</span>
									{/if}
								{:else}
									<span class="small"
										>{p.status ? statusLabel(p.status, e.title.kind) : 'Not yet'}</span
									>
								{/if}
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			{#if e.badges.length}
				<section class="badges">
					<h2>Badges</h2>
					<ul>
						{#each e.badges as b (b.key)}
							<li class:held={b.held}>
								<img src={b.picture} alt="" />
								<span class="words">
									<strong>{b.name}</strong>
									{#if b.about}<span class="small">{b.about}</span>{/if}
								</span>
								{#if b.held}<span class="pill ready-chip">Yours</span>{/if}
							</li>
						{/each}
					</ul>
				</section>
			{/if}
		</div>
	</div>

	{#if premiere}
		<Premiere
			event={e}
			{preview}
			target={head}
			ondone={() => {
				const was = preview;
				premiere = false;
				preview = false;
				if (!was) void load();
			}}
		/>
	{/if}
{/if}

<style>
	.head {
		position: relative;
		isolation: isolate;
		min-height: 62vh;
		display: flex;
		align-items: flex-end;
		padding: calc(var(--header-h) + 40px) var(--page-pad) 40px;
		overflow: hidden;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		z-index: -2;
		mask-image: linear-gradient(to top, transparent 0, #000 40%);
	}

	.backdrop img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
	}

	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(to right, color-mix(in srgb, var(--bg) 86%, transparent), transparent 70%),
			linear-gradient(to top, var(--bg) 0, transparent 60%);
	}

	.head-body {
		width: 100%;
		max-width: 1440px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 14px;
		min-width: 0;
	}

	.head-body.waiting {
		visibility: hidden;
	}

	.label {
		font-size: 0.82rem;
		font-weight: 800;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--event-accent);
	}

	.name-link {
		color: inherit;
		text-decoration: none;
		max-width: 100%;
	}

	.logo {
		display: block;
		max-width: min(520px, 82vw);
		max-height: 160px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.65));
	}

	h1 {
		font-size: clamp(2.4rem, 6vw, 4.6rem);
		letter-spacing: -0.045em;
	}

	.ready {
		color: var(--text);
	}

	.ready strong {
		margin-right: 4px;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}

	.actions .btn.primary {
		min-height: 46px;
		padding: 0 24px;
		background: var(--text);
		color: var(--bg);
	}

	.actions .btn.on-picture {
		min-height: 46px;
		background: color-mix(in srgb, var(--bg) 46%, transparent);
		backdrop-filter: var(--frost);
		border-color: var(--line-strong);
		color: var(--text);
	}

	.actions .more {
		width: 46px;
		padding: 0;
		justify-content: center;
	}

	.off {
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.body {
		padding-top: 8px;
	}

	.figures {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 10px;
		margin-bottom: 32px;
	}

	.figure {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px 16px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
	}

	.figure span {
		font-size: 0.78rem;
		color: var(--text-dim);
	}

	.figure strong {
		font-size: 1.4rem;
		font-variant-numeric: tabular-nums;
	}

	.list-head {
		flex-wrap: wrap;
		margin-bottom: 8px;
	}

	.all-seen {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-dim);
		margin: 16px 0 32px;
	}

	.group {
		margin: 18px 0 28px;
	}

	h3 {
		font-size: 0.82rem;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-dim);
		margin-bottom: 8px;
	}

	ol,
	ul {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	ol li {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 10px 0;
		border-bottom: 1px solid var(--line);
		min-width: 0;
	}

	ol li.seen .poster,
	ol li.seen .words {
		opacity: 0.55;
	}

	.n {
		flex: none;
		width: 22px;
		text-align: right;
		font-weight: 800;
		font-variant-numeric: tabular-nums;
		color: var(--text-dim);
	}

	.poster {
		flex: none;
		width: 46px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.poster img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.name {
		font-weight: 700;
		color: inherit;
		text-decoration: none;
	}

	.name:hover {
		text-decoration: underline;
	}

	.facts,
	.small {
		font-size: 0.8rem;
		color: var(--text-dim);
	}

	.note {
		font-size: 0.82rem;
		font-style: italic;
		color: var(--text-dim);
	}

	.pill.kind {
		flex: none;
	}

	.pill.kind:not(.optional) {
		color: var(--event-accent);
	}

	.pill.optional {
		border-style: dashed;
	}

	.state {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
		min-width: 110px;
	}

	.seen-mark {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.84rem;
		font-weight: 700;
		color: var(--accent);
	}

	.progress,
	.bar {
		display: block;
		width: 64px;
		height: 5px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.progress span,
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent);
	}

	.btn.add {
		min-height: 34px;
		padding: 0 12px;
	}

	section.about,
	section.cast,
	section.people,
	section.badges {
		margin: 32px 0;
	}

	section h2 {
		margin-bottom: 12px;
	}

	.about p {
		max-width: 64ch;
		line-height: 1.55;
		margin-bottom: 14px;
	}

	.cast ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
		gap: 14px;
	}

	.cast a {
		display: flex;
		flex-direction: column;
		gap: 4px;
		color: inherit;
		text-decoration: none;
		min-width: 0;
	}

	.face {
		display: block;
		aspect-ratio: 1;
		border-radius: 50%;
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.face img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.people ul {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.people li {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.who {
		flex: 1;
		min-width: 0;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ready-chip {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 40%, transparent);
	}

	.badges ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 12px;
	}

	.badges li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		min-width: 0;
	}

	.badges li:not(.held) img {
		filter: grayscale(1);
		opacity: 0.5;
	}

	.badges img {
		flex: none;
		width: 56px;
		height: 56px;
	}

	@media (max-width: 640px) {
		.head {
			min-height: 54vh;
			padding-top: calc(var(--header-h) + 20px);
		}

		.logo {
			max-height: 110px;
		}

		ol li {
			flex-wrap: wrap;
			gap: 10px;
		}

		.pill.kind {
			display: none;
		}

		.state {
			min-width: 0;
			margin-left: 36px;
		}

		.words {
			flex-basis: calc(100% - 100px);
		}
	}
</style>
