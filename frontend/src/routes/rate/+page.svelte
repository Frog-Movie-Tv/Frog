<script lang="ts">
	import { chooseStatus, statusEntries } from '#lib/add';
	import { api, img } from '#lib/api';
	import Empty from '#lib/components/Empty.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import ScoreSlider from '#lib/components/ScoreSlider.svelte';
	import Scores from '#lib/components/Scores.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import StarPicker from '#lib/components/StarPicker.svelte';
	import Stars from '#lib/components/Stars.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { formatScore, fromDisplay, toDisplay } from '#lib/format';
	import { InOrder } from '#lib/in-order';
	import { showFrom } from '#lib/menu.svelte';
	import {
		choiceForKey,
		choices,
		deckOf,
		ratedIn,
		takeBack,
		type DeckScope,
		type Step
	} from '#lib/rate';
	import { sliding } from '#lib/score-entry';
	import { settings } from '#lib/settings.svelte';
	import { titleHref } from '#lib/titles';
	import { toasts } from '#lib/toast.svelte';
	import {
		KIND_LABEL,
		statusLabel,
		type EntryRecord,
		type LibraryItem,
		type Status,
		type TitleScore
	} from '#lib/types';
	import { whenWatched } from '#lib/when.svelte';

	/*
	 * A library rated one card at a time: the title, what it was about and what
	 * other sites made of it, then a score and on to the next. Which titles are
	 * dealt, and what the buttons offer, live in `lib/rate.ts`.
	 *
	 * The cards still to rate are a queue, and every key and tap acts on its
	 * first card the moment it is pressed. A card on its way out is drawn
	 * apart from the queue (`flying`), so the next one is already in hand
	 * while the last one's stamp lands and it flies: nothing pressed during
	 * an animation is lost, and nothing is given to the wrong card. Saves go
	 * out one after another (`InOrder`), so an Undo never races the score it
	 * takes back. A save the server refuses brings its card back on top.
	 */

	let items = $state<LibraryItem[] | null>(null);
	let failed = $state<Failure | null>(null);
	let scope = $state<DeckScope>('finished');
	/** The cards still to go, the one in hand first. */
	let queue = $state<LibraryItem[]>([]);
	/** How many were dealt, for the bar along the top. */
	let dealt = $state(0);
	/** What each step did, newest last, so Undo can take it back. */
	let steps = $state<(Step<LibraryItem> & Undo)[]>([]);
	/** Cards on their way out: the stamp landing, then the flight. */
	let flying = $state<Flyer[]>([]);
	/** A card brought back by Undo comes in from the side it left by. */
	let returning = $state<-1 | 1 | null>(null);
	/** How far the card has been dragged sideways, in pixels. */
	let drag = $state(0);
	let dragging = $state(false);
	let scores = $state<Record<number, TitleScore[] | 'loading'>>({});
	let fixButton: HTMLButtonElement | undefined = $state();

	/** What it takes to take a step back on the server. */
	interface Undo {
		/** The rating before a score. */
		before?: number | null;
		/** The entry before a move or a removal, and how to put it back. */
		entries?: EntryRecord[];
		via?: 'restore' | 'unmark';
		/** The status the card showed before a move, for the card itself. */
		was?: Status;
	}

	interface Flyer {
		key: number;
		item: LibraryItem;
		/** Sideways, or up and away for a title taken out of the library. */
		dir: -1 | 1 | 0;
		stamp: string | null;
		stars: number | null;
		flying: boolean;
	}

	const saves = new InOrder();
	let nextKey = 0;

	const format = $derived(settings.current.scoreFormat);
	const options = $derived(choices(format, settings.current.halfStars));
	/** A hundred points, or ten with a decimal: slid to, not picked. */
	const slide = $derived(sliding(format));
	const card = $derived<LibraryItem | undefined>(queue[0]);
	/** The card in hand and the two under it, top first. */
	const pile = $derived(queue.slice(0, 3));
	const left = $derived(queue.length);
	const rated = $derived(ratedIn(steps));

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

	/** A fresh deck: its own count and its own history. */
	function deal(next: DeckScope) {
		scope = next;
		queue = deckOf(items ?? [], next);
		dealt = queue.length;
		steps = [];
		flying = [];
		returning = null;
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

	const moving = () => !window.matchMedia('(prefers-reduced-motion: reduce)').matches;

	/** How long a stamp holds, and how long a card takes to fly, in ms. */
	const HOLD = 260;
	const FLY = 320;

	/** The card in hand leaves the queue, and is drawn on its way out. */
	function send(it: LibraryItem, dir: -1 | 1 | 0, stamp: string | null, stars: number | null) {
		returning = null;
		queue = queue.filter((i) => i.id !== it.id);
		drag = 0;
		const key = ++nextKey;
		const motion = moving();
		if (!motion) return;
		flying = [...flying, { key, item: it, dir, stamp, stars, flying: !stamp }];
		const fly = () => {
			flying = flying.map((f) => (f.key === key ? { ...f, flying: true } : f));
			setTimeout(() => (flying = flying.filter((f) => f.key !== key)), FLY);
		};
		if (stamp) setTimeout(fly, HOLD);
		else setTimeout(() => (flying = flying.filter((f) => f.key !== key)), FLY);
	}

	function record(kind: Step<LibraryItem>['kind'], it: LibraryItem, left: boolean, undo: Undo) {
		const step = { key: ++nextKey, kind, item: it, left, ...undo };
		steps = [...steps, step];
		return step.key;
	}

	/** Give the card in hand a score, 0..100. */
	function rate(value: number) {
		const it = card;
		if (!it) return;
		const key = record('rate', it, true, { before: it.entry?.rating ?? null });
		send(it, 1, formatScore(value, format), format === 'star5' ? toDisplay(value, format) : null);
		const { done } = saves.run(() => api.track(it.id, { rating: value }));
		done
			.then(() => {
				if (it.entry) it.entry.rating = value;
			})
			.catch((e) => {
				// Refused: the card comes back to be rated again, and the count with it.
				if (!steps.some((s) => s.key === key)) return;
				giveBack(key, 1);
				toasts.error(e);
			});
	}

	function skip(dir: -1 | 1 = -1) {
		const it = card;
		if (!it) return;
		record('skip', it, true, {});
		send(it, dir, null, null);
	}

	/** One step back, in the queue and on the screen; not on the server. */
	function giveBack(key: number, from: -1 | 1) {
		const back = takeBack(queue, steps, key);
		if (!back.step) return null;
		flying = flying.filter((f) => f.item.id !== back.step!.item.id);
		queue = back.queue;
		steps = back.steps;
		if (back.step.left) {
			returning = from;
			setTimeout(() => (returning = null), 400);
		}
		drag = 0;
		return back.step;
	}

	/** Take a step back, the newest one unless named: the page's Undo, or a toast's. */
	function undo(key = steps.at(-1)?.key) {
		if (key === undefined) return;
		const step = giveBack(key, steps.find((s) => s.key === key)?.kind === 'skip' ? -1 : 1);
		if (!step || step.kind === 'skip') return;
		const it = step.item;
		const { done } = saves.run(async () => {
			if (step.kind === 'rate') {
				await api.track(it.id, { rating: step.before ?? null });
				if (it.entry) it.entry.rating = step.before ?? null;
			} else if (step.entries) {
				await api.bulk({ action: step.via ?? 'restore', entries: step.entries });
				if (it.entry && step.was) it.entry.status = step.was;
			}
		});
		done.catch((e) => toasts.error(e));
	}

	/** Move the card in hand to another status. It stays in hand: it may still want a score. */
	async function move(status: Status, leaves = false) {
		const it = card;
		if (!it || !it.entry || status === it.entry.status) return;
		let answer = null;
		if (status === 'completed') {
			answer = await whenWatched.ask(
				it.name,
				it.kind === 'tv' ? 'Every aired episode' : 'Marked as watched',
				{ film: it.kind === 'movie', reading: it.kind === 'manga' }
			);
			if (!answer) return;
		}
		const was = it.entry.status;
		try {
			const { done } = saves.run(() =>
				status === 'completed' && answer
					? api.bulk({ action: 'status', status: 'completed', title_ids: [it.id], ...answer })
					: api.bulk({
							action: 'status',
							status: status as Exclude<Status, 'completed'>,
							title_ids: [it.id]
						})
			);
			const { before } = await done;
			const key = record('move', it, leaves, {
				entries: before,
				via: status === 'completed' ? 'unmark' : 'restore',
				was
			});
			it.entry.status = status;
			if (leaves) send(it, 0, null, null);
			toasts.undoable(`Moved ${it.name} to ${statusLabel(status, it.kind)}`, () => undo(key));
		} catch (e) {
			toasts.error(e);
		}
	}

	/** Out of the library altogether: an import that brought in the wrong title. */
	async function remove() {
		const it = card;
		if (!it) return;
		try {
			const { done } = saves.run(() => api.bulk({ action: 'remove', title_ids: [it.id] }));
			const { before } = await done;
			const key = record('remove', it, true, { entries: before, via: 'restore' });
			send(it, 0, null, null);
			toasts.undoable(`Removed ${it.name}`, () => undo(key));
		} catch (e) {
			toasts.error(e);
		}
	}

	/** The status picker for the card in hand, from the "…" button or the S key. */
	async function pickStatus(e?: MouseEvent) {
		const it = card;
		if (!it?.entry || !fixButton) return;
		const status = await chooseStatus(it, fixButton, {
			event: e,
			current: it.entry.status,
			title: it.name
		});
		if (status) void move(status);
	}

	/** Everything a card can be fixed with, behind one button. */
	function fixes(e: MouseEvent) {
		const it = card;
		if (!it?.entry || !fixButton) return;
		showFrom(
			fixButton,
			{
				title: it.name,
				items: () => [
					{
						label: it.kind === 'manga' ? 'Not read yet' : 'Not watched yet',
						icon: 'eye-off',
						run: () => void move('planned', true)
					},
					{
						label: 'Change status',
						icon: 'filter',
						key: 'S',
						children: () =>
							statusEntries(it.kind, (s) => void move(s), { current: it.entry?.status })
					},
					'rule',
					{
						label: 'Remove from library',
						icon: 'trash',
						danger: true,
						key: 'Del',
						run: () => void remove()
					}
				]
			},
			e
		);
	}

	function onKey(e: KeyboardEvent) {
		if (e.ctrlKey || e.metaKey || e.altKey) return;
		const target = e.target as HTMLElement | null;
		if (target?.closest('input, textarea, select, [role="dialog"], [role="slider"], [role="menu"]'))
			return;
		const pick = slide ? null : choiceForKey(e.key, options.length);
		if (pick !== null) {
			e.preventDefault();
			rate(fromDisplay(options[pick], format) ?? 0);
		} else if (e.key === 'ArrowLeft') {
			e.preventDefault();
			skip(-1);
		} else if (e.key === 'ArrowRight') {
			e.preventDefault();
			skip(1);
		} else if (e.key === 'Backspace' || e.key === 'z') {
			e.preventDefault();
			undo();
		} else if (e.key === 's' || e.key === 'S') {
			e.preventDefault();
			void pickStatus();
		} else if (e.key === 'Delete') {
			e.preventDefault();
			void remove();
		}
	}

	// Drag a card sideways and let go past the line to skip it. A pointer
	// reports far more often than the screen draws, so the card moves once a
	// frame, to where the pointer last was, and it is moved directly: going
	// through the page's state would redraw the card sixty times a second,
	// which a phone cannot keep up with. The state catches up on letting go.
	const THROW = 110;
	let startX = 0;
	let lastX = 0;
	let frame = 0;
	let held: HTMLElement | null = null;

	function down(e: PointerEvent) {
		if (e.button !== 0 || (e.target as HTMLElement).closest('a, button')) return;
		held = e.currentTarget as HTMLElement;
		dragging = true;
		startX = lastX = e.clientX;
		held.setPointerCapture(e.pointerId);
	}

	function move_(e: PointerEvent) {
		if (!dragging) return;
		lastX = e.clientX;
		frame ||= requestAnimationFrame(() => {
			frame = 0;
			if (!dragging || !held) return;
			const dx = lastX - startX;
			held.style.transform = `translateX(${dx}px) rotate(${dx / 22}deg)`;
			held.style.setProperty('--pull', String(Math.min(1, Math.abs(dx) / THROW)));
		});
	}

	function up() {
		if (!dragging) return;
		dragging = false;
		cancelAnimationFrame(frame);
		frame = 0;
		const moved = lastX - startX;
		const card = held;
		held = null;
		card?.style.removeProperty('--pull');
		if (Math.abs(moved) > THROW) {
			skip(moved < 0 ? -1 : 1);
		} else {
			drag = 0;
			// Back to where the page has it: the transform it drew is the same
			// as before the drag, so it would not write it again.
			if (card) card.style.transform = placed(0);
		}
	}

	/** Where a card in the pile sits: the top one follows the hand. */
	function placed(depth: number) {
		if (depth > 0) return `translateY(${depth * 14}px) scale(${1 - depth * 0.05})`;
		return `translateX(${drag}px) rotate(${drag / 22}deg)`;
	}

	/** Where a card on its way out is: holding for its stamp, then gone. */
	function away(f: Flyer) {
		if (!f.flying) return 'scale(1.02)';
		if (f.dir === 0) return 'translateY(-115%) scale(0.92)';
		return `translateX(${f.dir * 125}%) translateY(-4%) rotate(${f.dir * 12}deg)`;
	}

	const year = (d: string | null) => (d ? d.slice(0, 4) : null);

	function facts(it: LibraryItem): string {
		const bits = [KIND_LABEL[it.kind]];
		const y = year(it.release_date);
		if (y) bits.push(y);
		if (it.entry) bits.push(statusLabel(it.entry.status, it.kind));
		return bits.join(' · ');
	}
</script>

<svelte:head>
	<title>Rate · Frog</title>
</svelte:head>

<svelte:window onkeydown={onKey} />

{#snippet face(it: LibraryItem, top: boolean)}
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
				{:else if it.kind === 'manga' && it.watched_episodes}
					<span class="watched">
						Read {it.watched_episodes}{it.total_episodes ? ` of ${it.total_episodes}` : ''} chapters
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

		<a class="open hit" href={titleHref(it)} tabindex={top ? 0 : -1}>
			Open the title <Icon name="chevron-right" size={14} />
		</a>
	</div>
{/snippet}

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
		<div class="segmented" role="group" aria-label="Which titles">
			<button class:on={scope === 'finished'} onclick={() => deal('finished')}>Finished</button>
			<button class:on={scope === 'all'} onclick={() => deal('all')}>Everything</button>
		</div>
	</div>

	{#if failed}
		<LoadFailed {failed} onretry={load} />
	{:else if !items}
		<Spinner label="Dealing your library" />
	{:else if !card && !flying.length}
		<Empty
			art="done"
			title={rated ? 'All rated' : 'Nothing to rate'}
			hint={scope === 'finished'
				? 'Every title you finished has a rating.'
				: 'Every title in your library has a rating.'}
		>
			{#if steps.length}
				<button class="btn" onclick={() => undo()}><Icon name="undo" size={15} /> Undo</button>
			{/if}
			<a class="btn primary" href="/library">Back to Library</a>
		</Empty>
	{:else}
		<div class="progress" aria-hidden="true">
			<i style:width="{dealt ? ((dealt - left) / dealt) * 100 : 0}%"></i>
		</div>

		<div class="table">
			{#each [...pile].reverse() as it (it.id)}
				{@const depth = pile.indexOf(it)}
				<article
					class="card"
					class:top={depth === 0}
					class:dragging={depth === 0 && dragging}
					class:back={depth === 0 && returning !== null}
					style:--from={returning ?? 1}
					style:transform={placed(depth)}
					style:z-index={10 - depth}
					style:--tint={it.accent.shade}
					aria-hidden={depth > 0}
					onpointerdown={depth === 0 ? down : undefined}
					onpointermove={depth === 0 ? move_ : undefined}
					onpointerup={depth === 0 ? up : undefined}
					onpointercancel={depth === 0 ? up : undefined}
				>
					{@render face(it, depth === 0)}
					{#if depth === 0}
						<span class="stamp skip" aria-hidden="true">Skip</span>
						<button
							class="fix hit"
							bind:this={fixButton}
							aria-label="Fix this title"
							title="Fix this title"
							aria-haspopup="menu"
							onclick={fixes}
						>
							<Icon name="more" size={18} />
						</button>
					{/if}
				</article>
			{/each}
			{#each flying as f (f.key)}
				<article
					class="card flyer"
					class:gone={f.flying}
					style:transform={away(f)}
					style:--tint={f.item.accent.shade}
					aria-hidden="true"
				>
					{@render face(f.item, false)}
					{#if f.stamp}
						<span class="stamp">
							{#if f.stars !== null}<Stars value={f.stars} size={34} />{:else}{f.stamp}{/if}
						</span>
					{/if}
				</article>
			{/each}
		</div>

		{#if card}
			{#if slide}
				<div class="typed">
					<ScoreSlider {format} autofocus reset={card.id} onsubmit={(v) => rate(v)} />
				</div>
			{:else if format === 'star5'}
				<div class="filled">
					<StarPicker
						halves={settings.current.halfStars}
						size={44}
						reset={card.id}
						onpick={(stars) => rate(fromDisplay(stars, format) ?? 0)}
					/>
				</div>
			{:else}
				<div class="choices" role="group" aria-label="Your rating">
					{#each options as option, i (option)}
						<button class="choice" onclick={() => rate(fromDisplay(option, format) ?? 0)}>
							<span>{formatScore(fromDisplay(option, format), format)}</span>
							{#if i < 10}<kbd>{i === 9 ? 0 : i + 1}</kbd>{/if}
						</button>
					{/each}
				</div>
			{/if}
		{/if}

		<div class="moves">
			<button class="btn ghost" onclick={() => undo()} disabled={!steps.length}>
				<Icon name="undo" size={15} /> Undo
			</button>
			<button class="btn ghost" disabled={!card} onclick={() => skip(-1)}>
				Skip <Icon name="chevron-right" size={15} />
			</button>
		</div>
	{/if}
</div>

<style>
	/* A card dragged past the edge never widens the page: a wider page moves
	   everything on it, every frame of the drag. */
	.page {
		max-width: 720px;
		overflow-x: clip;
	}

	.sub {
		margin-top: 6px;
		font-size: 0.92rem;
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
	/* The cards under the top one sit up to two places lower (`placed`), so
	   the pile keeps that much room under it for the stars and buttons. */
	.table {
		position: relative;
		height: clamp(300px, calc(100dvh - 420px), 560px);
		margin-bottom: 46px;
	}

	.card {
		position: absolute;
		inset: 0;
		overflow: hidden;
		border-radius: var(--radius-lg);
		border: 1px solid var(--line);
		/* Solid: the cards are stacked, and the next one must never read
		   through the one on top. */
		background: color-mix(in srgb, var(--tint, var(--bg-float)) 55%, var(--bg-float));
		box-shadow: var(--shadow-3);
		touch-action: pan-y;
		user-select: none;
		transform-origin: 50% 90%;
		/* Each card is its own layer, so moving one never repaints it. */
		will-change: transform;
		transition:
			transform 340ms cubic-bezier(0.2, 0.8, 0.2, 1),
			opacity 340ms ease;
	}

	/* Under dark words on a light theme, only a hint of the poster's shade. */
	:global(:root[data-tone='light']) .card {
		background: color-mix(in srgb, var(--tint, var(--bg-float)) 14%, var(--bg-float));
	}

	.card {
		animation: join 260ms ease-out backwards;
	}

	.card.top {
		cursor: grab;
	}

	/* Brought back by Undo: in from the side it went. */
	.card.back {
		animation: return 360ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	.card:not(.top) {
		pointer-events: none;
	}

	.card.dragging {
		cursor: grabbing;
		transition: none;
		/* A lighter shadow while it moves: less to draw each frame. */
		box-shadow: var(--shadow-2);
	}

	.card.gone {
		opacity: 0;
	}

	/* A card on its way out sits over the pile and goes where it was sent;
	   it was already drawn, so it does not fade in. */
	.card.flyer {
		z-index: 20;
		pointer-events: none;
		animation: none;
	}

	/* The card's fixes: one quiet button in its corner. */
	.fix {
		position: absolute;
		z-index: 21;
		top: 12px;
		right: 12px;
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: color-mix(in srgb, var(--bg-float) 80%, transparent);
		color: var(--text-dim);
		cursor: pointer;
	}

	.fix:hover {
		color: var(--text);
		background: var(--bg-hover);
	}

	/* A card new to the pile fades in where it lies: only opacity, so it never
	   fights the move that puts it there. */
	@keyframes join {
		from {
			opacity: 0;
		}
	}

	@keyframes return {
		from {
			opacity: 0;
			transform: translateX(calc(var(--from) * 110%)) rotate(calc(var(--from) * 10deg));
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

	/* The score just given, stamped across the card it was given to, which
	   then carries it away. */
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
		animation: stamp 240ms cubic-bezier(0.3, 1.5, 0.5, 1) backwards;
	}

	@keyframes stamp {
		from {
			opacity: 0;
			transform: translate(-50%, -50%) rotate(-6deg) scale(1.7);
		}
	}

	/* While a card is dragged, the side it would go says so, as far as the
	   drag has gone (`--pull`, set as it moves). */
	.stamp.skip {
		opacity: calc((var(--pull, 0) - 0.2) * 1.25);
		transition: none;
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
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 8px;
		margin-bottom: 14px;
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

	.filled {
		display: flex;
		justify-content: center;
		margin-bottom: 10px;
	}

	/* Smileys are big enough to tell apart at a glance. */
	.choice span {
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
			height: clamp(270px, calc(100dvh - 450px), 520px);
			margin-bottom: 42px;
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

		.stamp {
			font-size: 2rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.card,
		.progress i {
			transition: none;
		}

		.card,
		.card.back,
		.stamp {
			animation: none;
		}
	}
</style>
