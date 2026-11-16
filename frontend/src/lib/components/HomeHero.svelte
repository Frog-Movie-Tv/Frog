<script lang="ts" module>
	import type {
		AiringEntry,
		FeaturedTitle,
		HomeRecommendation,
		LibraryItem,
		UpNextEntry
	} from '#lib/types';

	/** One thing the hero can show. */
	export type Slide =
		| { kind: 'rec'; rec: HomeRecommendation }
		| { kind: 'featured'; item: LibraryItem; rank: number }
		| { kind: 'new'; entry: AiringEntry }
		| { kind: 'today'; hero: UpNextEntry };

	/** At most this many slides. */
	export const MAX_SLIDES = 8;
	/** How long one slide stays before the next. */
	export const SLIDE_MS = 8000;

	/**
	 * What the hero rotates through: recommendations to you not answered yet,
	 * what is trending that you do not have, and what airs this week from what
	 * you follow, never what you are in the middle of. They take turns, one of
	 * each, so a long list of one never pushes the others out. With none of
	 * them, it is the one hero Home always had.
	 */
	export function slidesOf(
		recommended: HomeRecommendation[],
		featured: FeaturedTitle[],
		thisWeek: AiringEntry[],
		hero: UpNextEntry | null
	): Slide[] {
		const queues: Slide[][] = [
			recommended.map((rec) => ({ kind: 'rec', rec })),
			featured.map((f) => ({ kind: 'featured', item: f.item, rank: f.rank })),
			thisWeek.map((entry) => ({ kind: 'new', entry }))
		];
		const seen: number[] = [];
		const slides: Slide[] = [];
		while (slides.length < MAX_SLIDES && queues.some((q) => q.length)) {
			for (const queue of queues) {
				// Skip what another kind already showed: one title, one slide.
				let next = queue.shift();
				while (next && seen.includes(itemOf(next).id)) next = queue.shift();
				if (!next || slides.length >= MAX_SLIDES) continue;
				seen.push(itemOf(next).id);
				slides.push(next);
			}
		}
		if (!slides.length && hero) slides.push({ kind: 'today', hero });
		return slides;
	}

	export function itemOf(s: Slide): LibraryItem {
		switch (s.kind) {
			case 'rec':
				return s.rec.item;
			case 'featured':
				return s.item;
			case 'new':
				return s.entry.item;
			case 'today':
				return s.hero.item;
		}
	}

	export const slideKey = (s: Slide): string =>
		s.kind === 'rec' ? `rec:${s.rec.id}` : `${s.kind}:${itemOf(s).id}`;
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import { fade } from 'svelte/transition';
	import { addFrom } from '#lib/add';
	import { api, img } from '#lib/api';
	import Avatar from './Avatar.svelte';
	import Icon from './Icon.svelte';
	import { airTime, episodeCode, prettyDate, relativeDate, runtime, year } from '#lib/format';
	import { avatarOf, personName } from '#lib/people.svelte';
	import type { HeroStyle } from '#lib/settings.svelte';
	import { episodeRange, firstMoment } from '#lib/thisweek';
	import { titleHref } from '#lib/titles';
	import { toasts } from '#lib/toast.svelte';

	interface Props {
		slides: Slide[];
		style: HeroStyle;
		/** A recommendation was answered here: it leaves the rotation. */
		onanswered: (id: number) => void;
		/** A trending title was added here: it leaves the rotation. */
		onadded: (titleId: number) => void;
		/** Taken back: Home reads itself again. */
		onchanged: () => void;
	}

	let { slides, style, onanswered, onadded, onchanged }: Props = $props();

	let index = $state(0);
	const count = $derived(slides.length);
	// A slide answered away leaves the index past the end: back inside.
	const at = $derived(count ? Math.min(index, count - 1) : 0);
	const slide = $derived(slides[at] as Slide | undefined);
	const item = $derived(slide ? itemOf(slide) : null);

	interface Picture {
		src: string;
		/** Wide screens get the full-size picture: a backdrop fills them. */
		srcset: string | null;
	}

	function picture(s: Slide | undefined): Picture | null {
		if (!s) return null;
		const i = itemOf(s);
		if (style === 'backdrop' && i.backdrop_path) {
			const p = i.backdrop_path;
			return {
				src: img(p, 'w1280')!,
				srcset: `${img(p, 'w780')} 780w, ${img(p, 'w1280')} 1280w, ${img(p, 'original')} 1920w`
			};
		}
		if (style === 'poster' && i.poster_path)
			return { src: img(i.poster_path, 'w780')!, srcset: null };
		return null;
	}

	// --- pictures, one fading in over the last ------------------------------
	// The picture showing stays, fully there, until the next has loaded and
	// faded in over it, so a turn never passes through an empty screen.
	interface Layer extends Picture {
		id: number;
		shown: boolean;
	}
	let layers = $state<Layer[]>([]);
	let layerId = 0;

	$effect(() => {
		const p = picture(slide);
		untrack(() => {
			if (!p) {
				layers = [];
				return;
			}
			if (layers.at(-1)?.src === p.src) return;
			// One under it at most: a picture never shown is not worth fading from.
			const under = layers.filter((l) => l.shown).slice(-1);
			layers = [...under, { ...p, id: ++layerId, shown: false }];
		});
	});

	/** Shown once its picture is there, whether loaded now or long ago. */
	function appear(node: HTMLImageElement, layer: Layer) {
		const show = () => (layer.shown = true);
		if (node.complete && node.naturalWidth) requestAnimationFrame(show);
		else {
			node.addEventListener('load', show, { once: true });
			node.addEventListener('error', show, { once: true });
		}
	}

	/** Faded in: what was under it can go. */
	function settled(layer: Layer, e: TransitionEvent) {
		if (e.propertyName !== 'opacity' || !layer.shown) return;
		layers = layers.filter((l) => l.id >= layer.id);
	}

	// --- turning -----------------------------------------------------------
	let hovered = $state(false);
	let focused = $state(false);
	let hidden = $state(typeof document !== 'undefined' && document.hidden);
	const still =
		typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
	const paused = $derived(hovered || focused || hidden || still);

	function go(to: number) {
		if (count) index = (to + count) % count;
	}

	// The hero turns when the active dot has filled (`onanimationend`), so
	// pausing the fill pauses the turn, to the frame, and a click starts both
	// again from the slide now shown.

	$effect(() => {
		const onVisible = () => (hidden = document.hidden);
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	});

	// The next picture is fetched while this one shows, so turning never waits.
	$effect(() => {
		if (count < 2) return;
		const next = picture(slides[(at + 1) % count]);
		if (!next) return;
		const ahead = new Image();
		if (next.srcset) {
			ahead.sizes = '100vw';
			ahead.srcset = next.srcset;
		}
		ahead.src = next.src;
	});

	function keys(e: KeyboardEvent) {
		if (count < 2 || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight')) return;
		e.preventDefault();
		go(at + (e.key === 'ArrowRight' ? 1 : -1));
	}

	// A swipe sideways on a touch screen turns it.
	let touch: { x: number; y: number } | null = null;
	function down(e: PointerEvent) {
		touch = e.pointerType === 'touch' ? { x: e.clientX, y: e.clientY } : null;
	}
	function up(e: PointerEvent) {
		if (!touch) return;
		const dx = e.clientX - touch.x;
		const dy = e.clientY - touch.y;
		touch = null;
		if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.5) go(at + (dx < 0 ? 1 : -1));
	}

	// --- what today's hero says -------------------------------------------
	const next = $derived(slide?.kind === 'today' ? slide.hero.next : null);
	const caughtUp = $derived(!!next && !next.aired);
	const fresh = $derived(
		!!item &&
			slide?.kind === 'today' &&
			item.kind === 'tv' &&
			item.watched_episodes === 0 &&
			!caughtUp
	);
	const label = $derived.by(() => {
		if (!slide || !item) return '';
		if (slide.kind === 'rec') return `Recommended by ${personName(slide.rec.from)}`;
		if (slide.kind === 'featured') return 'Trending this week';
		if (slide.kind === 'new') {
			const day = slide.entry.air_date ? relativeDate(slide.entry.air_date) : 'This week';
			return `${day} · ${airTime(firstMoment(slide.entry))}`;
		}
		return caughtUp
			? 'Caught up'
			: fresh
				? 'Start watching'
				: item.kind === 'manga' && item.entry?.status === 'watching'
					? 'Continue reading'
					: next || item.entry?.status === 'watching'
						? 'Continue watching'
						: // Nothing in progress: the hero is simply the newest addition.
							'Recently added';
	});

	// --- answering a recommendation ---------------------------------------
	let busy = $state(false);

	async function accept(rec: HomeRecommendation, e: MouseEvent) {
		busy = true;
		try {
			if (!rec.item.entry) {
				const done = await addFrom(rec.item, e.currentTarget as HTMLElement, e, onchanged);
				if (!done) return;
			}
			await api.answerRecommendation(rec.id, 'added');
			onanswered(rec.id);
		} catch (err) {
			toasts.error(err);
		} finally {
			busy = false;
		}
	}

	/** Words leave quickly; the next slide's arrive a beat later, in order. */
	const LEAVE_MS = 160;

	async function add(featured: LibraryItem, e: MouseEvent) {
		busy = true;
		try {
			const done = await addFrom(featured, e.currentTarget as HTMLElement, e, onchanged);
			if (done) onadded(featured.id);
		} catch (err) {
			toasts.error(err);
		} finally {
			busy = false;
		}
	}

	async function decline(rec: HomeRecommendation) {
		busy = true;
		try {
			await api.answerRecommendation(rec.id, 'dismissed');
			onanswered(rec.id);
			toasts.undoable(`Dismissed ${rec.item.name}`, async () => {
				await api.answerRecommendation(rec.id, null);
				onchanged();
			});
		} catch (err) {
			toasts.error(err);
		} finally {
			busy = false;
		}
	}
</script>

{#if slide && item}
	<header
		class="hero"
		class:pictured={layers.length > 0}
		class:paused
		class:still
		role={count > 1 ? 'region' : undefined}
		aria-roledescription={count > 1 ? 'carousel' : undefined}
		aria-label={count > 1 ? 'Featured' : undefined}
		onpointerenter={(e) => {
			if (e.pointerType === 'mouse') hovered = true;
		}}
		onpointerleave={() => (hovered = false)}
		onfocusin={() => (focused = true)}
		onfocusout={(e) => {
			const to = e.relatedTarget as Node | null;
			if (!to || !(e.currentTarget as HTMLElement).contains(to)) focused = false;
		}}
		onpointerdown={down}
		onpointerup={up}
		onpointercancel={() => (touch = null)}
		onkeydown={keys}
		style:--slide-ms="{SLIDE_MS}ms"
		style:--leave-ms="{still ? 0 : LEAVE_MS}ms"
	>
		{#each layers as layer (layer.id)}
			<div class="backdrop" class:shown={layer.shown} ontransitionend={(e) => settled(layer, e)}>
				<img
					class:poster={style === 'poster'}
					src={layer.src}
					srcset={layer.srcset}
					sizes={layer.srcset ? '100vw' : undefined}
					alt=""
					fetchpriority={layer.id === 1 ? 'high' : undefined}
					use:appear={layer}
				/>
			</div>
		{/each}
		<div class="veil" aria-hidden="true"></div>

		<div class="stage">
			{#key slideKey(slide)}
				<div
					class="hero-body"
					out:fade={{ duration: still ? 0 : LEAVE_MS }}
					role={count > 1 ? 'group' : undefined}
					aria-roledescription={count > 1 ? 'slide' : undefined}
					aria-label={count > 1 ? `${at + 1} of ${count}` : undefined}
				>
					{#if slide.kind === 'rec'}
						<span class="hero-label from">
							<Avatar src={avatarOf(slide.rec.from)} name={personName(slide.rec.from)} size={26} />
							{label}
						</span>
					{:else if slide.kind === 'featured'}
						<span class="hero-label trending">
							<span class="rank" aria-label="Number {slide.rank}">#{slide.rank}</span>
							{label}
						</span>
					{:else}
						<span class="hero-label" class:when={slide.kind === 'new'}>{label}</span>
					{/if}

					<a class="name-link" href={titleHref(item)}>
						{#if item.logo_path}
							<img class="logo" src={img(item.logo_path, 'w500')} alt={item.name} />
						{:else}
							<h1>{item.name}</h1>
						{/if}
					</a>

					<p class="facts">
						{#if item.tmdb_vote}
							<span class="vote" aria-label="Rated {item.tmdb_vote.toFixed(1)} on TMDB">
								<Icon name="star" size={13} filled />
								{item.tmdb_vote.toFixed(1)}
							</span>
						{/if}
						{#if year(item.release_date)}<span>{year(item.release_date)}</span>{/if}
						{#if item.kind === 'movie' && item.runtime}
							<span>{runtime(item.runtime)}</span>
						{:else if item.kind === 'tv' && item.season_count}
							<span>{item.season_count} {item.season_count === 1 ? 'season' : 'seasons'}</span>
						{/if}
						{#if item.genres.length}<span>{item.genres.slice(0, 3).join(' · ')}</span>{/if}
					</p>

					{#if slide.kind === 'rec'}
						{#if slide.rec.note}<p class="note">“{slide.rec.note}”</p>{/if}
						<div class="hero-actions">
							<button
								class="btn primary"
								disabled={busy}
								aria-haspopup={slide.rec.item.entry ? undefined : 'menu'}
								onclick={(e) => slide?.kind === 'rec' && accept(slide.rec, e)}
							>
								<Icon name={slide.rec.item.entry ? 'check' : 'plus'} size={15} />
								{slide.rec.item.entry ? 'Accept' : 'Add to library'}
							</button>
							<button
								class="btn on-picture"
								disabled={busy}
								onclick={() => slide?.kind === 'rec' && decline(slide.rec)}
							>
								Not for me
							</button>
						</div>
					{:else if slide.kind === 'featured'}
						{#if item.overview}<p class="overview">{item.overview}</p>{/if}
						<div class="hero-actions">
							<button
								class="btn primary"
								disabled={busy}
								aria-haspopup="menu"
								onclick={(e) => slide?.kind === 'featured' && add(slide.item, e)}
							>
								<Icon name="plus" size={15} />
								Add to library
							</button>
							<a class="btn on-picture" href={titleHref(item)}>
								<Icon name="info" size={17} />
								More info
							</a>
						</div>
					{:else if slide.kind === 'new'}
						<p class="upnext">
							<strong>{episodeRange(slide.entry)}</strong>
							{#if slide.entry.episodes.length === 1}
								{slide.entry.episodes[0].name ?? ''}
							{:else}
								{slide.entry.episodes.length} episodes
							{/if}
						</p>
						<div class="hero-actions">
							<a class="btn primary" href={titleHref(item)}>View show</a>
						</div>
					{:else}
						{#if next}
							<p class="upnext">
								{#if caughtUp}
									Next episode {next.air_date ? prettyDate(next.air_date) : 'to be announced'}
									{#if next.air_date}· {airTime(next.air_at)}{/if}
								{:else}
									<strong>{episodeCode(next.season_number, next.episode_number)}</strong>
									{next.name ?? ''}
								{/if}
							</p>
						{/if}
						<div class="hero-actions">
							{#if caughtUp}
								<a class="btn" href={titleHref(item)}>View show</a>
							{:else}
								<a class="btn primary" href={titleHref(item)}>
									<Icon name="play" size={14} filled />
									{fresh
										? 'Start watching'
										: item.kind === 'manga'
											? `Chapter ${item.watched_episodes + 1}`
											: next
												? 'Next episode'
												: 'Open'}
								</a>
							{/if}
						</div>
					{/if}
				</div>
			{/key}
		</div>

		{#if count > 1}
			<div class="controls">
				<button class="turn hit" aria-label="Previous" onclick={() => go(at - 1)}>
					<Icon name="chevron-left" size={18} />
				</button>
				<div class="dots" role="group" aria-label="Choose a slide">
					{#each slides as s, i (slideKey(s))}
						<button
							class="dot"
							class:on={i === at}
							aria-label="{i + 1} of {count}: {itemOf(s).name}"
							aria-current={i === at ? 'true' : undefined}
							onclick={() => go(i)}
						>
							<span class="mark">
								{#if i === at}
									<!-- Its filling is the slide's time: when it is full, the next. -->
									<span class="fill" onanimationend={() => go(at + 1)}></span>
								{/if}
							</span>
						</button>
					{/each}
				</div>
				<button class="turn hit" aria-label="Next" onclick={() => go(at + 1)}>
					<Icon name="chevron-right" size={18} />
				</button>
			</div>
		{/if}
	</header>
{/if}

<style>
	/* The screen belongs to one title: its picture fills it and fades into
	   the light behind the page, so the hero has no bottom edge. */
	.hero {
		position: relative;
		isolation: isolate;
		min-height: min(56vh, 480px);
		display: flex;
		align-items: flex-end;
		padding: calc(var(--header-h) + 48px) var(--page-pad) 56px;
		overflow: hidden;
		touch-action: pan-y;
	}

	.hero.pictured {
		min-height: 88vh;
		padding-bottom: 72px;
	}

	/* Each picture is a layer that fades in over the last. The layer holds
	   the fade into the page and stays put; only the picture inside it moves,
	   so the hero's lower edge never shows. */
	.backdrop {
		position: absolute;
		inset: 0;
		z-index: -2;
		overflow: hidden;
		/* The picture fades out itself, so its edge never shows as a line where
		   the page begins. */
		mask-image: linear-gradient(to top, transparent 0, #000 34%);
		opacity: 0;
		transition: opacity 900ms cubic-bezier(0.4, 0, 0.2, 1);
	}

	.backdrop.shown {
		opacity: 1;
	}

	.backdrop img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
		transform-origin: 60% 30%;
	}

	.backdrop img.poster {
		object-position: center 12%;
	}

	/* A slow settle for as long as it stays: still pictures, but never a
	   still screen. */
	.backdrop.shown img {
		animation: drift calc(var(--slide-ms) + 3s) cubic-bezier(0.25, 0.6, 0.3, 1) forwards;
	}

	/* Shade behind the words, on the left where they sit, fading out with
	   the picture. */
	.veil {
		position: absolute;
		inset: 0;
		z-index: -1;
		background:
			linear-gradient(to right, color-mix(in srgb, var(--bg) 84%, transparent), transparent 64%),
			linear-gradient(
				to top,
				color-mix(in srgb, var(--bg) 70%, transparent) 0,
				color-mix(in srgb, var(--bg) 38%, transparent) 32%,
				transparent 56%
			);
		mask-image: linear-gradient(to top, transparent 0, #000 28%);
	}

	.hero-label {
		font-size: 1rem;
		font-weight: 700;
		color: var(--text);
		opacity: 0.85;
	}

	.hero-label.from {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		opacity: 1;
	}

	.hero-label.when {
		display: inline-block;
	}

	.hero-label.trending {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		opacity: 1;
	}

	/* Its place on the list, as a badge: the one thing on the slide that
	   says why it is there. */
	.rank {
		display: inline-grid;
		place-items: center;
		min-width: 34px;
		height: 26px;
		padding: 0 8px;
		border-radius: 7px;
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.85rem;
		font-weight: 800;
		font-variant-numeric: tabular-nums;
		letter-spacing: -0.01em;
		box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--accent) 80%, transparent);
	}

	.hero-label.when::first-letter {
		text-transform: uppercase;
	}

	.hero .facts,
	.hero .upnext {
		color: var(--text);
	}

	/* The leaving slide's words and the arriving ones share one place, so
	   the hero never jumps in height between them. */
	.stage {
		display: grid;
		max-width: 1440px;
		margin: 0 auto;
		width: 100%;
	}

	.hero-body {
		grid-area: 1 / 1;
		align-self: end;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
	}

	/* Each line arrives a moment after the one above it, once the last
	   slide's words are gone. */
	.hero-body > :global(*) {
		animation: words-in 620ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--leave-ms) + var(--n, 0) * 60ms);
	}
	.hero-body > :global(:nth-child(2)) {
		--n: 1;
	}
	.hero-body > :global(:nth-child(3)) {
		--n: 2;
	}
	.hero-body > :global(:nth-child(4)) {
		--n: 3;
	}
	.hero-body > :global(:nth-child(n + 5)) {
		--n: 4;
	}

	.name-link {
		color: inherit;
		text-decoration: none;
	}

	.logo {
		display: block;
		max-width: min(620px, 82vw);
		max-height: 200px;
		object-fit: contain;
		object-position: left bottom;
		filter: drop-shadow(0 4px 18px rgba(0, 0, 0, 0.65));
	}

	/* On a dark theme a black logo would vanish into the page; a faint light
	   halo keeps its outline without changing a coloured one. */
	:global(:root[data-tone='dark']) .logo {
		filter: drop-shadow(0 0 1px rgb(255 255 255 / 0.45))
			drop-shadow(0 0 14px rgb(255 255 255 / 0.12)) drop-shadow(0 4px 18px rgb(0 0 0 / 0.5));
	}

	/* On a light theme the hero fades to a pale page, where a white logo
	   would lose its edges: a dark outline and a closer shadow keep them. */
	:global(:root[data-tone='light']) .logo {
		filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6)) drop-shadow(0 1px 3px rgb(0 0 0 / 0.45))
			drop-shadow(0 6px 22px rgb(0 0 0 / 0.55));
	}

	.facts {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 14px;
		font-size: 0.88rem;
		color: var(--text-dim);
		text-shadow: 0 1px 8px rgb(0 0 0 / 0.35);
	}

	.vote {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.vote :global(svg) {
		color: #f5c518;
	}

	.upnext {
		font-size: 0.95rem;
		color: var(--text-dim);
	}

	.upnext strong {
		color: var(--accent);
		font-variant-numeric: tabular-nums;
		margin-right: 6px;
	}

	.note {
		max-width: 54ch;
		font-size: 1rem;
		line-height: 1.5;
		color: var(--text);
	}
	/* Three lines at most: the picture is the point. */
	.overview {
		max-width: 54ch;
		font-size: 0.95rem;
		line-height: 1.5;
		color: var(--text);
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.hero h1 {
		font-size: clamp(3rem, 7vw, 5.6rem);
		letter-spacing: -0.045em;
	}

	.hero-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-top: 6px;
	}

	/* The one thing to do, in the page's own ink, as a TV app's Play is. */
	.hero-actions .btn.primary {
		min-height: 50px;
		padding: 0 30px;
		background: var(--text);
		color: var(--bg);
		font-size: 1rem;
		box-shadow: 0 10px 30px -10px rgba(0, 0, 0, 0.6);
	}

	.hero-actions .btn.on-picture {
		min-height: 50px;
		padding: 0 24px;
		background: color-mix(in srgb, var(--bg) 46%, transparent);
		backdrop-filter: var(--frost);
		border-color: var(--line-strong);
		color: var(--text);
		font-size: 1rem;
	}

	/* Turning lives in one corner, beside the dots, away from the words. */
	.controls {
		position: absolute;
		z-index: 2;
		bottom: 24px;
		right: max(var(--page-pad), calc((100% - 1440px) / 2 + var(--page-pad)));
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.turn {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: color-mix(in srgb, var(--bg) 50%, transparent);
		backdrop-filter: var(--frost);
		color: var(--text);
		cursor: pointer;
		opacity: 0.55;
		transition:
			opacity var(--fast) var(--ease),
			background var(--fast) var(--ease);
	}

	.hero:hover .turn,
	.turn:focus-visible {
		opacity: 1;
	}

	.turn:hover {
		background: color-mix(in srgb, var(--bg) 30%, transparent);
	}

	.dots {
		display: flex;
		align-items: center;
	}

	/* A wide target, a small mark. */
	.dot {
		display: grid;
		place-items: center;
		height: 28px;
		padding: 0 4px;
		border: 0;
		background: none;
		cursor: pointer;
	}

	.mark {
		display: block;
		width: 8px;
		height: 8px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--text) 36%, transparent);
		overflow: hidden;
		transition:
			width 300ms var(--ease),
			background var(--fast) var(--ease);
	}

	.dot:hover .mark {
		background: color-mix(in srgb, var(--text) 60%, transparent);
	}

	/* The shown slide's mark is a bar that fills for as long as it stays. */
	.dot.on .mark {
		width: 32px;
		background: color-mix(in srgb, var(--text) 30%, transparent);
	}

	.fill {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--text);
		transform-origin: left center;
		animation: fill var(--slide-ms) linear forwards;
	}

	.hero.paused .fill {
		animation-play-state: paused;
	}

	/* Nothing turns by itself: the mark is simply full. */
	.hero.still .fill {
		animation: none;
	}

	@keyframes fill {
		from {
			transform: scaleX(0);
		}
	}

	@keyframes drift {
		from {
			transform: scale(1.06);
		}
		to {
			transform: scale(1);
		}
	}

	@keyframes words-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	@media (hover: none) {
		.turn {
			display: none;
		}
	}

	@media (max-width: 720px) {
		.hero {
			min-height: 48vh;
			padding-top: calc(var(--header-h) + 24px);
		}

		.hero.pictured {
			min-height: 72vh;
			padding-bottom: 48px;
		}

		.logo {
			max-height: 120px;
		}

		.controls {
			left: 0;
			right: 0;
			bottom: 14px;
			justify-content: center;
		}

		.turn {
			display: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.backdrop,
		.backdrop.shown img,
		.hero-body > :global(*) {
			animation: none;
			transition: none;
		}
	}
</style>
