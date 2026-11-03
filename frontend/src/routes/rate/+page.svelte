<script lang="ts">
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import ScoreField from '#lib/components/ScoreField.svelte';
	import Scores from '#lib/components/Scores.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import Stars from '#lib/components/Stars.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { formatScore, fromDisplay, toDisplay } from '#lib/format';
	import { choiceForKey, choices, deckOf, type DeckScope } from '#lib/rate';
	import { typed } from '#lib/score-entry';
	import { settings } from '#lib/settings.svelte';
	import { titleHref } from '#lib/titles';
	import { toasts } from '#lib/toast.svelte';
	import { STATUS_LABEL, type LibraryItem, type TitleScore } from '#lib/types';

	/*
	 * A library rated one card at a time: the title, what it was about and what
	 * other sites made of it, then a score and on to the next. Which titles are
	 * dealt, and what the buttons offer, live in `lib/rate.ts`.
	 */

	let items = $state<LibraryItem[] | null>(null);
	let failed = $state<Failure | null>(null);
	let scope = $state<DeckScope>('seen');
	let deck = $state<LibraryItem[]>([]);
	let at = $state(0);
	let rated = $state(0);
	/** What each step did, so Undo can take it back. */
	let steps = $state<{ index: number; id: number; before: number | null; skipped: boolean }[]>([]);
	/** The card on its way out: which way, and the score stamped on it. */
	let leaving = $state<{ dir: -1 | 1; stamp: string | null } | null>(null);
	/** The score just given, shown over the deck for a moment. A count, so the
	 *  same score twice in a row still pops. */
	let flash = $state<{ text: string; stars: number | null; n: number } | null>(null);
	let flashes = 0;
	let flashTimer: ReturnType<typeof setTimeout> | undefined;
	/** How far the card has been dragged sideways, in pixels. */
	let drag = $state(0);
	let dragging = $state(false);
	let scores = $state<Record<number, TitleScore[] | 'loading'>>({});

	const format = $derived(settings.current.scoreFormat);
	const options = $derived(choices(format, settings.current.halfStars));
	/** A hundred points, or ten with a decimal: typed, not picked. */
	const typing = $derived(typed(format));
	const card = $derived<LibraryItem | undefined>(deck[at]);
	/** The card in hand and the two under it, top first. */
	const pile = $derived(deck.slice(at, at + 3));
	const left = $derived(Math.max(0, deck.length - at));

	function load() {
		failed = null;
		api
			.library()
			.then((r) => {
				items = r.items;
				deal(scope);
			})
			.catch(
				(e) =>
					(failed = failure(e, {
						title: 'Could not load your library',
						hint: 'Try again in a minute.'
					}))
			);
	}

	$effect(() => {
		load();
	});

	function deal(next: DeckScope) {
		scope = next;
		deck = deckOf(items ?? [], next);
		at = 0;
		steps = [];
	}

	// The card in hand and the next two have their scores waiting.
	$effect(() => {
		for (const it of pile) void scoresOf(it.id);
	});

	async function scoresOf(id: number) {
		if (scores[id]) return;
		scores[id] = 'loading';
		try {
			scores[id] = (await api.scores(id)).ratings;
		} catch {
			scores[id] = [];
		}
	}

	/** Other sites' scores, or TMDB's own while there are none. */
	function shown(it: LibraryItem): TitleScore[] {
		const s = scores[it.id];
		if (Array.isArray(s) && s.length) return s;
		return it.tmdb_vote ? [{ source: 'tmdb', score: it.tmdb_vote * 10, votes: null }] : [];
	}

	const moving = () =>
		settings.current.animations !== 'off' &&
		!window.matchMedia('(prefers-reduced-motion: reduce)').matches;

	let busy = false;

	/** The card leaves that way, and the next one comes up. */
	function onward(dir: -1 | 1, stamp: string | null, stars: number | null = null) {
		leaving = { dir, stamp };
		if (stamp) {
			flash = { text: stamp, stars, n: ++flashes };
			clearTimeout(flashTimer);
			flashTimer = setTimeout(() => (flash = null), moving() ? 700 : 400);
		}
		setTimeout(
			() => {
				leaving = null;
				drag = 0;
				at += 1;
				busy = false;
			},
			moving() ? 340 : 0
		);
	}

	/** Give the card in hand a score, 0..100. */
	async function rate(value: number) {
		const it = card;
		if (!it || busy) return;
		busy = true;
		steps = [...steps, { index: at, id: it.id, before: it.entry?.rating ?? null, skipped: false }];
		rated += 1;
		onward(1, formatScore(value, format), format === 'star5' ? toDisplay(value, format) : null);
		try {
			await api.track(it.id, { rating: value });
			if (it.entry) it.entry.rating = value;
		} catch (e) {
			toasts.error(e);
		}
	}

	function skip(dir: -1 | 1 = -1) {
		const it = card;
		if (!it || busy) return;
		busy = true;
		steps = [...steps, { index: at, id: it.id, before: it.entry?.rating ?? null, skipped: true }];
		onward(dir, null);
	}

	async function undo() {
		const last = steps.at(-1);
		if (!last || busy) return;
		steps = steps.slice(0, -1);
		at = last.index;
		drag = 0;
		if (last.skipped) return;
		rated -= 1;
		try {
			await api.track(last.id, { rating: last.before });
			const it = deck.find((d) => d.id === last.id);
			if (it?.entry) it.entry.rating = last.before;
		} catch (e) {
			toasts.error(e);
		}
	}

	function onKey(e: KeyboardEvent) {
		if (e.ctrlKey || e.metaKey || e.altKey) return;
		const target = e.target as HTMLElement | null;
		if (target?.closest('input, textarea, select, [role="dialog"]')) return;
		const pick = typing ? null : choiceForKey(e.key, options.length);
		if (pick !== null) {
			e.preventDefault();
			void rate(fromDisplay(options[pick], format) ?? 0);
		} else if (e.key === 'ArrowLeft') {
			e.preventDefault();
			skip(-1);
		} else if (e.key === 'ArrowRight') {
			e.preventDefault();
			skip(1);
		} else if (e.key === 'Backspace' || e.key === 'z') {
			e.preventDefault();
			void undo();
		}
	}

	// Drag a card sideways and let go past the line to skip it.
	const THROW = 110;
	let startX = 0;

	function down(e: PointerEvent) {
		if (busy || (e.target as HTMLElement).closest('a, button')) return;
		dragging = true;
		startX = e.clientX;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function move(e: PointerEvent) {
		if (dragging) drag = e.clientX - startX;
	}

	function up() {
		if (!dragging) return;
		dragging = false;
		if (Math.abs(drag) > THROW) skip(drag < 0 ? -1 : 1);
		else drag = 0;
	}

	/** Where a card in the pile sits: the top one follows the hand. */
	function placed(depth: number) {
		if (depth > 0) return `translateY(${depth * 14}px) scale(${1 - depth * 0.05})`;
		if (leaving) return `translateX(${leaving.dir * 130}%) rotate(${leaving.dir * 14}deg)`;
		return `translateX(${drag}px) rotate(${drag / 22}deg)`;
	}

	const year = (d: string | null) => (d ? d.slice(0, 4) : null);

	function facts(it: LibraryItem): string {
		const bits = [it.kind === 'tv' ? 'Show' : 'Film'];
		const y = year(it.release_date);
		if (y) bits.push(y);
		if (it.entry) bits.push(STATUS_LABEL[it.entry.status]);
		return bits.join(' · ');
	}
</script>

<svelte:head>
	<title>Rate · Frog</title>
</svelte:head>

<svelte:window onkeydown={onKey} />

<div class="page">
	<div class="page-head">
		<div>
			<h1>Rate your library</h1>
			{#if items}
				<p class="sub faint">
					{#if card}{left} to go{:else}Nothing left to rate{/if}
					{#if rated}· {rated} rated{/if}
				</p>
			{/if}
		</div>
		<div class="switch" role="group" aria-label="Which titles">
			<button class:on={scope === 'seen'} onclick={() => deal('seen')}>Seen</button>
			<button class:on={scope === 'all'} onclick={() => deal('all')}>Everything</button>
		</div>
	</div>

	{#if failed}
		<LoadFailed {failed} onretry={load} />
	{:else if !items}
		<Spinner label="Dealing your library" />
	{:else if !card}
		<Empty
			art="done"
			title={rated ? 'All rated' : 'Nothing to rate'}
			hint={scope === 'seen'
				? 'Every title you have seen has a rating.'
				: 'Every title in your library has a rating.'}
		>
			{#if steps.length}
				<button class="btn" onclick={undo}><Icon name="undo" size={15} /> Undo</button>
			{/if}
			<a class="btn primary" href="/library">Back to Library</a>
		</Empty>
	{:else}
		<div class="progress" aria-hidden="true">
			<i style:width="{deck.length ? (at / deck.length) * 100 : 0}%"></i>
		</div>

		<div class="table">
			{#each [...pile].reverse() as it (it.id)}
				{@const depth = pile.indexOf(it)}
				<article
					class="card"
					class:top={depth === 0}
					class:dragging={depth === 0 && dragging}
					class:gone={depth === 0 && leaving}
					style:transform={placed(depth)}
					style:z-index={10 - depth}
					style:--tint={it.accent.shade}
					aria-hidden={depth > 0}
					onpointerdown={depth === 0 ? down : undefined}
					onpointermove={depth === 0 ? move : undefined}
					onpointerup={depth === 0 ? up : undefined}
					onpointercancel={depth === 0 ? up : undefined}
				>
					{#if it.backdrop_path}
						<img class="backdrop" src={img(it.backdrop_path, 'w780')} alt="" draggable="false" />
					{/if}
					<div class="veil"></div>

					<div class="body">
						<div class="top-row">
							{#if it.poster_path}
								<img class="poster" src={img(it.poster_path, 'w342')} alt="" draggable="false" />
							{/if}
							<div class="title">
								<span class="facts">{facts(it)}</span>
								<h2>{it.name}</h2>
								{#if it.genres.length}
									<span class="genres">{it.genres.slice(0, 3).join(', ')}</span>
								{/if}
								{#if it.kind === 'tv' && it.total_episodes}
									<span class="watched">
										Watched {it.watched_episodes} of {it.total_episodes} episodes
									</span>
								{/if}
							</div>
						</div>

						{#if it.overview}<p class="overview">{it.overview}</p>{/if}

						<div class="scores">
							{#if scores[it.id] === 'loading' && !shown(it).length}
								<span class="faint">Finding scores…</span>
							{:else if shown(it).length}
								<Scores scores={shown(it)} />
							{:else}
								<span class="faint">No scores yet</span>
							{/if}
						</div>

						<a class="open hit" href={titleHref(it)} tabindex={depth === 0 ? 0 : -1}>
							Open the title <Icon name="chevron-right" size={14} />
						</a>
					</div>

					{#if depth === 0 && !leaving && Math.abs(drag) > 24}
						<span class="stamp skip" style:opacity={Math.min(1, Math.abs(drag) / THROW)}>
							Skip
						</span>
					{/if}
				</article>
			{/each}

			{#if flash}
				{#key flash.n}
					<span class="stamp" aria-hidden="true">
						{#if flash.stars !== null}<Stars
								value={flash.stars}
								size={34}
							/>{:else}{flash.text}{/if}
					</span>
				{/key}
			{/if}
		</div>

		{#if typing}
			<div class="typed">
				<ScoreField {format} autofocus reset={card.id} onsubmit={(v) => rate(v)} />
			</div>
		{:else}
			<div
				class="choices"
				role="group"
				aria-label="Your rating"
				data-count={options.length}
				data-format={format}
			>
				{#each options as option, i (option)}
					<button class="choice" onclick={() => rate(fromDisplay(option, format) ?? 0)}>
						{#if format === 'star5'}
							<Stars value={option} label={formatScore(fromDisplay(option, format), format)} />
						{:else}
							<span>{formatScore(fromDisplay(option, format), format)}</span>
						{/if}
						{#if i < 10}<kbd>{i === 9 ? 0 : i + 1}</kbd>{/if}
					</button>
				{/each}
			</div>
		{/if}

		<div class="moves">
			<button class="btn ghost" onclick={undo} disabled={!steps.length}>
				<Icon name="undo" size={15} /> Undo
			</button>
			<button class="btn ghost" onclick={() => skip(-1)}>
				Skip <Icon name="chevron-right" size={15} />
			</button>
		</div>
	{/if}
</div>

<style>
	.page {
		max-width: 720px;
	}

	.sub {
		margin-top: 6px;
		font-size: 0.92rem;
	}

	.switch {
		display: flex;
		gap: 2px;
		padding: 3px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev);
	}

	.switch button {
		padding: 6px 14px;
		border: 0;
		border-radius: var(--radius-pill);
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

	.progress {
		height: 4px;
		margin: 4px 0 22px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.progress i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width 340ms var(--ease);
	}

	/* The pile: cards on top of each other, the top one in hand. */
	/* Tall enough for the card, short enough that the scores below it are
	   always on screen: the deck is never scrolled to be used. */
	.table {
		position: relative;
		height: clamp(300px, calc(100dvh - 400px), 560px);
		margin-bottom: 22px;
	}

	.card {
		position: absolute;
		inset: 0;
		overflow: hidden;
		border-radius: var(--radius-lg);
		border: 1px solid var(--line);
		background: color-mix(in srgb, var(--tint, var(--bg-elev)) 55%, var(--bg-elev));
		box-shadow: var(--shadow-3);
		touch-action: pan-y;
		user-select: none;
		transform-origin: 50% 90%;
		transition:
			transform 340ms cubic-bezier(0.2, 0.8, 0.2, 1),
			opacity 340ms ease;
	}

	.card.top {
		cursor: grab;
		animation: deal-in 300ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	.card:not(.top) {
		pointer-events: none;
	}

	.card.dragging {
		cursor: grabbing;
		transition: none;
	}

	.card.gone {
		opacity: 0;
	}

	@keyframes deal-in {
		from {
			opacity: 0;
			transform: translateY(14px) scale(0.95);
		}
	}

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		opacity: 0.34;
		pointer-events: none;
	}

	.veil {
		position: absolute;
		inset: 0;
		background: linear-gradient(
			to top,
			var(--bg-elev) 30%,
			color-mix(in srgb, var(--bg-elev) 55%, transparent)
		);
	}

	.body {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 16px;
		height: 100%;
		padding: 24px;
		overflow-y: auto;
	}

	.top-row {
		display: flex;
		gap: 18px;
		align-items: flex-end;
	}

	.poster {
		flex: none;
		width: 120px;
		aspect-ratio: 2 / 3;
		object-fit: cover;
		border-radius: var(--radius);
		box-shadow: var(--shadow-2);
		pointer-events: none;
	}

	.title {
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
	}

	.title h2 {
		font-size: clamp(1.4rem, 4vw, 2rem);
	}

	.facts,
	.watched {
		font-size: 0.82rem;
		color: var(--text-dim);
	}

	.genres {
		font-size: 0.86rem;
		color: var(--text-faint);
	}

	.overview {
		display: -webkit-box;
		-webkit-line-clamp: 6;
		line-clamp: 6;
		-webkit-box-orient: vertical;
		overflow: hidden;
		color: var(--text-dim);
		font-size: 0.95rem;
	}

	.scores {
		margin-top: auto;
	}

	.open {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		align-self: flex-start;
		color: var(--accent);
		font-size: 0.86rem;
		font-weight: 600;
	}

	/* The score just given, stamped over the middle of the deck while the card
	   flies off: big, a little turned, and gone again. */
	.stamp {
		position: absolute;
		z-index: 20;
		top: 50%;
		left: 50%;
		padding: 10px 26px;
		border-radius: var(--radius);
		border: 3px solid var(--accent);
		color: var(--accent);
		background: color-mix(in srgb, var(--bg) 82%, transparent);
		font-family: var(--font-display);
		font-size: 2.6rem;
		font-weight: 800;
		line-height: 1.1;
		white-space: nowrap;
		pointer-events: none;
		transform: translate(-50%, -50%) rotate(-6deg);
		box-shadow: var(--shadow-3);
		animation: stamp 700ms cubic-bezier(0.3, 1.4, 0.5, 1) forwards;
	}

	@keyframes stamp {
		0% {
			opacity: 0;
			transform: translate(-50%, -50%) rotate(-6deg) scale(1.6);
		}
		25% {
			opacity: 1;
			transform: translate(-50%, -50%) rotate(-6deg) scale(1);
		}
		75% {
			opacity: 1;
		}
		100% {
			opacity: 0;
			transform: translate(-50%, -50%) rotate(-6deg) scale(0.96);
		}
	}

	/* While a card is dragged, the side it would go says so. */
	.stamp.skip {
		top: 22px;
		left: 22px;
		padding: 6px 16px;
		border-color: var(--text-dim);
		color: var(--text-dim);
		font-size: 1.4rem;
		transform: rotate(-8deg);
		box-shadow: none;
		animation: none;
	}

	.typed {
		margin-bottom: 14px;
	}

	.choices {
		display: grid;
		grid-template-columns: repeat(var(--cols, 5), minmax(0, 1fr));
		gap: 8px;
		margin-bottom: 14px;
	}

	.choices[data-count='3'] {
		--cols: 3;
	}

	.choice {
		position: relative;
		display: grid;
		place-items: center;
		min-height: 52px;
		padding: 8px 4px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg-elev);
		color: var(--text);
		font-size: 1.05rem;
		font-weight: 700;
		cursor: pointer;
		transition:
			background var(--fast) var(--ease),
			transform var(--fast) var(--ease);
	}

	.choice:hover {
		background: var(--accent);
		border-color: transparent;
		color: var(--accent-ink);
	}

	.choice:active {
		transform: scale(0.95);
	}

	/* Stars are drawn, half stars too, and stay on one line whatever their
	   count; smileys are big enough to tell apart at a glance. */
	.choices[data-format='star5'] .choice :global(.stars svg) {
		width: 14px;
		height: 14px;
	}

	.choices[data-format='smiley3'] .choice span {
		font-size: 1.9rem;
		line-height: 1;
	}

	.choice kbd {
		position: absolute;
		top: 4px;
		right: 6px;
		font-family: var(--font-mono);
		font-size: 0.62rem;
		font-weight: 500;
		opacity: 0.55;
	}

	@media (hover: none) and (pointer: coarse) {
		.choice kbd {
			display: none;
		}
	}

	.moves {
		display: flex;
		justify-content: space-between;
		gap: 12px;
	}

	@media (max-width: 600px) {
		.page-head {
			flex-wrap: nowrap;
			align-items: center;
		}

		.page-head h1 {
			font-size: 1.5rem;
		}

		.table {
			height: clamp(280px, calc(100dvh - 430px), 520px);
			margin-bottom: 16px;
		}

		.progress {
			margin-bottom: 16px;
		}

		.body {
			padding: 18px;
			gap: 12px;
		}

		.poster {
			width: 88px;
		}

		.overview {
			-webkit-line-clamp: 5;
			line-clamp: 5;
			font-size: 0.9rem;
		}

		.choice {
			min-height: 46px;
			font-size: 0.95rem;
		}

		.choices[data-format='star5'] .choice :global(.stars svg) {
			width: 11px;
			height: 11px;
		}

		.stamp {
			font-size: 2rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.card,
		.progress i {
			transition: none;
		}

		.card.top,
		.stamp {
			animation: none;
		}
	}
</style>
