<script lang="ts" module>
	import type { AiringEntry, HomeRecommendation, LibraryItem, UpNextEntry } from '#lib/types';

	/** One thing the hero can show. */
	export type Slide =
		| { kind: 'rec'; rec: HomeRecommendation }
		| { kind: 'new'; entry: AiringEntry }
		| { kind: 'today'; hero: UpNextEntry };

	/** At most this many slides, recommendations first. */
	export const MAX_SLIDES = 6;
	/** How long one slide stays before the next. */
	export const SLIDE_MS = 8000;

	/**
	 * What the hero rotates through: recommendations to you not answered yet,
	 * then what airs this week from what you follow, never what you are in the
	 * middle of. With neither, it is the one hero Home always had.
	 */
	export function slidesOf(
		recommended: HomeRecommendation[],
		thisWeek: AiringEntry[],
		hero: UpNextEntry | null
	): Slide[] {
		const seen: number[] = [];
		const slides: Slide[] = [];
		for (const rec of recommended) {
			if (slides.length >= MAX_SLIDES || seen.includes(rec.item.id)) continue;
			seen.push(rec.item.id);
			slides.push({ kind: 'rec', rec });
		}
		for (const entry of thisWeek) {
			if (slides.length >= MAX_SLIDES || seen.includes(entry.item.id)) continue;
			seen.push(entry.item.id);
			slides.push({ kind: 'new', entry });
		}
		if (!slides.length && hero) slides.push({ kind: 'today', hero });
		return slides;
	}

	export const itemOf = (s: Slide): LibraryItem =>
		s.kind === 'rec' ? s.rec.item : s.kind === 'new' ? s.entry.item : s.hero.item;

	export const slideKey = (s: Slide): string =>
		s.kind === 'rec' ? `rec:${s.rec.id}` : `${s.kind}:${itemOf(s).id}`;
</script>

<script lang="ts">
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
		/** Taken back: Home reads itself again. */
		onchanged: () => void;
	}

	let { slides, style, onanswered, onchanged }: Props = $props();

	let index = $state(0);
	const count = $derived(slides.length);
	// A slide answered away leaves the index past the end: back inside.
	const at = $derived(count ? Math.min(index, count - 1) : 0);
	const slide = $derived(slides[at] as Slide | undefined);
	const item = $derived(slide ? itemOf(slide) : null);

	function picture(s: Slide | undefined): string | null {
		if (!s) return null;
		const i = itemOf(s);
		if (style === 'backdrop' && i.backdrop_path) return img(i.backdrop_path, 'w1280');
		if (style === 'poster' && i.poster_path) return img(i.poster_path, 'w780');
		return null;
	}
	const src = $derived(picture(slide));

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

	$effect(() => {
		if (paused || count < 2) return;
		// Started again from the slide now shown, so a click never lands on a
		// slide about to turn.
		void at;
		const timer = setInterval(() => go(at + 1), SLIDE_MS);
		return () => clearInterval(timer);
	});

	$effect(() => {
		const onVisible = () => (hidden = document.hidden);
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	});

	// The next picture is fetched while this one shows, so turning never waits.
	$effect(() => {
		if (count < 2) return;
		const next = picture(slides[(at + 1) % count]);
		if (next) new Image().src = next;
	});

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
		class:pictured={!!src}
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
	>
		{#key slideKey(slide)}
			{#if src}
				<img
					class="backdrop"
					class:poster={style === 'poster'}
					{src}
					alt=""
					fetchpriority={at === 0 ? 'high' : undefined}
				/>
			{/if}
		{/key}
		<div class="veil" aria-hidden="true"></div>

		{#key slideKey(slide)}
			<div
				class="hero-body"
				role={count > 1 ? 'group' : undefined}
				aria-roledescription={count > 1 ? 'slide' : undefined}
				aria-label={count > 1 ? `${at + 1} of ${count}` : undefined}
			>
				{#if slide.kind === 'rec'}
					<span class="hero-label from">
						<Avatar src={avatarOf(slide.rec.from)} name={personName(slide.rec.from)} size={26} />
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
					{#if year(item.release_date)}<span>{year(item.release_date)}</span>{/if}
					{#if item.kind === 'movie' && item.runtime}
						<span>{runtime(item.runtime)}</span>
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

		{#if count > 1}
			<button class="turn prev hit" aria-label="Previous" onclick={() => go(at - 1)}>
				<Icon name="chevron-left" size={24} />
			</button>
			<button class="turn next hit" aria-label="Next" onclick={() => go(at + 1)}>
				<Icon name="chevron-right" size={24} />
			</button>
			<div class="dots" role="group" aria-label="Choose a slide">
				{#each slides as s, i (slideKey(s))}
					<button
						class="dot"
						class:on={i === at}
						aria-label="{i + 1} of {count}: {itemOf(s).name}"
						aria-current={i === at ? 'true' : undefined}
						onclick={() => go(i)}
					></button>
				{/each}
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

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: center 20%;
		z-index: -2;
		/* The picture fades out itself, so its edge never shows as a line where
		   the page begins. */
		mask-image: linear-gradient(to top, transparent 0, #000 34%);
		animation: slide-in 700ms ease backwards;
	}

	.backdrop.poster {
		object-position: center 12%;
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

	.hero-label.when::first-letter {
		text-transform: uppercase;
	}

	.hero .facts,
	.hero .upnext {
		color: var(--text);
	}

	.hero-body {
		max-width: 1440px;
		margin: 0 auto;
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
		animation: words-in 520ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
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
		gap: 14px;
		font-size: 0.88rem;
		color: var(--text-dim);
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

	/* Arrows at either side, out of the way until the pointer comes near. */
	.turn {
		position: absolute;
		top: 50%;
		translate: 0 -50%;
		z-index: 2;
		display: grid;
		place-items: center;
		width: 48px;
		height: 48px;
		border: 1px solid var(--line);
		border-radius: 50%;
		background: color-mix(in srgb, var(--bg) 50%, transparent);
		backdrop-filter: var(--frost);
		color: var(--text);
		cursor: pointer;
		opacity: 0;
		transition: opacity var(--fast) var(--ease);
	}

	.turn.prev {
		left: 16px;
	}

	.turn.next {
		right: 16px;
	}

	.hero:hover .turn,
	.turn:focus-visible {
		opacity: 1;
	}

	.dots {
		position: absolute;
		z-index: 2;
		bottom: 24px;
		right: max(var(--page-pad), calc((100% - 1440px) / 2 + var(--page-pad)));
		display: flex;
		gap: 4px;
	}

	/* A wide target, a small mark. */
	.dot {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
	}

	.dot::after {
		content: '';
		width: 8px;
		height: 8px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--text) 40%, transparent);
		transition:
			width var(--fast) var(--ease),
			background var(--fast) var(--ease);
	}

	.dot.on::after {
		width: 20px;
		background: var(--text);
	}

	@keyframes slide-in {
		from {
			opacity: 0;
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

		.dots {
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
		.hero-body {
			animation: none;
		}
	}
</style>
