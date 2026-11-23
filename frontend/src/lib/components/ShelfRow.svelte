<script lang="ts">
	import { flushSync, type Snippet } from 'svelte';
	import { img } from '#lib/api';
	import { readSoon } from '#lib/bookcase';
	import { spineHeight, spineWidth, stand } from '#lib/books';
	import { plural, runtime } from '#lib/format';
	import { contextMenu, type MenuSource } from '#lib/menu.svelte';
	import { morph } from '#lib/morph.svelte';
	import type { Shelf } from '#lib/types';
	import Icon from './Icon.svelte';
	import Plank from './Plank.svelte';

	interface Props {
		shelf: Shelf;
		/** False while the shelves are being arranged: the shelf is not a link. */
		link?: boolean;
		/** What a right-click, or a hold, offers. */
		menu?: MenuSource;
		/** Its place on the page, so the shelves fill one after another. */
		index?: number;
		/** Something to hold it by, at the front of its label. */
		children?: Snippet;
	}

	let { shelf, link = true, menu, index = 0, children }: Props = $props();

	const KIND = {
		watchlist: { icon: 'bookmark', label: 'Watchlist' },
		smart: { icon: 'sparkle', label: 'Smart' },
		manual: { icon: 'layers', label: 'Hand-picked' }
	} as const;

	/** Face-out posters and their gap, as the stylesheet draws them. */
	const FACE = 112;
	const GAP = 12;

	let width = $state(0);
	let phone = $state(false);
	$effect(() => {
		const query = matchMedia('(max-width: 720px)');
		phone = query.matches;
		const changed = () => (phone = query.matches);
		query.addEventListener('change', changed);
		return () => query.removeEventListener('change', changed);
	});

	const books = $derived(shelf.books ?? []);
	/** On a phone the shelf scrolls sideways, so everything it was sent stands. */
	const standing = $derived(
		phone
			? {
					faces: books.slice(0, 3),
					spines: books.slice(3),
					rest: Math.max(0, shelf.count - books.length)
				}
			: stand(books, shelf.count, Math.max(width - 36, 0), FACE, GAP)
	);
	/** Three of what did not fit, lying flat. */
	const pile = $derived.by(() => {
		const after = books.slice(
			standing.faces.length + standing.spines.length,
			3 + standing.faces.length + standing.spines.length
		);
		return after.length ? after : books.slice(-3);
	});

	const left = $derived(shelf.minutes?.left ?? 0);
	const total = $derived(shelf.minutes?.total ?? 0);
	const watched = $derived(total > 0 ? 1 - left / total : 0);
	const RING = 2 * Math.PI * 9;

	// The shelf opened zooms into its page: its name becomes the heading and
	// its books fly to their places there. Only this shelf's are named, and
	// only for that one navigation.
	const key = $derived(`shelf${shelf.id}`);
	const named = $derived(link && morph.key === key);

	let root: HTMLElement | undefined = $state();

	/** The zoom starts from this shelf, there and on the way back. */
	$effect(() => {
		if (!named || !root) return;
		const r = root.getBoundingClientRect();
		const html = document.documentElement.style;
		html.setProperty('--zoom-x', `${Math.round(r.left + r.width / 2)}px`);
		html.setProperty('--zoom-y', `${Math.round(r.top + r.height * 0.55)}px`);
	});

	function open() {
		if (!link) return;
		readSoon(shelf.id);
		morph.key = key;
		// Named now, before the transition takes its picture of this page.
		flushSync();
	}

	/** A poster leans toward the pointer. Mice only: a finger has no hover. */
	function tilt(node: HTMLElement) {
		let held: HTMLElement | null = null;
		const still = matchMedia('(prefers-reduced-motion: reduce)');
		const letGo = () => {
			held?.classList.remove('tilt');
			held?.style.removeProperty('--rx');
			held?.style.removeProperty('--ry');
			held = null;
		};
		const move = (e: PointerEvent) => {
			if (e.pointerType !== 'mouse' || still.matches) return;
			const face = (e.target as HTMLElement).closest<HTMLElement>('.face');
			if (face !== held) letGo();
			if (!face) return;
			held = face;
			const r = face.getBoundingClientRect();
			const x = (e.clientX - r.left) / r.width - 0.5;
			const y = (e.clientY - r.top) / r.height - 0.5;
			face.classList.add('tilt');
			face.style.setProperty('--ry', `${(x * 22).toFixed(1)}deg`);
			face.style.setProperty('--rx', `${(-y * 16 + 4).toFixed(1)}deg`);
		};
		node.addEventListener('pointermove', move);
		node.addEventListener('pointerleave', letGo);
		return {
			destroy() {
				node.removeEventListener('pointermove', move);
				node.removeEventListener('pointerleave', letGo);
			}
		};
	}
</script>

<svelte:element
	this={link ? 'a' : 'div'}
	bind:this={root}
	class="shelfrow"
	href={link ? `/shelves/${shelf.id}` : undefined}
	aria-label={link ? shelf.name : undefined}
	use:contextMenu={link ? menu : undefined}
	style:--glow={shelf.accent ?? undefined}
	style:--row={Math.min(index, 8)}
	onpointerenter={() => link && readSoon(shelf.id)}
	onfocus={() => link && readSoon(shelf.id)}
	onclick={open}
	role={link ? undefined : 'group'}
>
	<div class="bay" bind:clientWidth={width}>
		<div class="stand" use:tilt>
			{#each standing.faces as book, k (book.id)}
				<span
					class="face"
					style:--k={k}
					style:view-transition-name={named ? `book-${book.id}` : undefined}
				>
					{#if book.poster_path}
						<img src={img(book.poster_path, 'w342')} alt="" loading="lazy" draggable="false" />
					{:else}
						<span class="blank">{book.name}</span>
					{/if}
					<span class="peek">{book.name}</span>
				</span>
			{/each}
			{#if standing.spines.length}
				<span class="books">
					{#each standing.spines as book, k (book.id)}
						<span
							class="spine"
							class:lean={!standing.rest && k === standing.spines.length - 1 && k > 1}
							style:--k={standing.faces.length + k}
							style:--sw="{spineWidth(book.minutes)}px"
							style:--sh={spineHeight(book.id)}
							style:background-image={book.poster_path
								? `url(${img(book.poster_path, 'w185')})`
								: undefined}
							style:view-transition-name={named ? `book-${book.id}` : undefined}
						>
							<span class="t">{book.name}</span>
							<span class="peek"
								>{book.name}{#if book.minutes}<span class="len">{runtime(book.minutes)}</span
									>{/if}</span
							>
						</span>
					{/each}
				</span>
			{/if}
			{#if standing.rest}
				<span class="pile" title={plural(standing.rest, 'more title')}>
					<span class="more">+{standing.rest}</span>
					{#each pile as book (book.id)}
						<i
							style:background-image={book.poster_path
								? `url(${img(book.poster_path, 'w185')})`
								: undefined}
						></i>
					{/each}
				</span>
			{/if}
			{#if shelf.count === 0}
				<span class="vacant"
					>{shelf.kind === 'smart' ? 'Nothing matches yet' : 'Nothing here yet'}</span
				>
			{/if}
			<span class="bookend" aria-hidden="true"></span>
		</div>
		<Plank sheen={shelf.kind === 'smart'}>
			<span class="holder">
				{@render children?.()}
				<strong
					class="name"
					style:view-transition-name={named ? `shelf-name-${shelf.id}` : undefined}
					>{shelf.name}</strong
				>
				<span class="kind"
					><Icon name={KIND[shelf.kind].icon} size={13} /> {KIND[shelf.kind].label}</span
				>
			</span>
			<span class="facts">
				{#if shelf.kind === 'watchlist' && total > 0}
					<span class="time">
						<svg class="ring" viewBox="0 0 22 22" aria-hidden="true">
							<circle class="track" cx="11" cy="11" r="9" />
							<circle
								class="done"
								cx="11"
								cy="11"
								r="9"
								stroke-dasharray="{(RING * watched).toFixed(1)} {RING.toFixed(1)}"
							/>
						</svg>
						{#if left > 0}<span><b>{runtime(left)}</b> to go</span>{:else}<span>All watched</span
							>{/if}
					</span>
				{/if}
				<span class="count">{plural(shelf.count, 'title')}</span>
				{#if link}<span class="open">Open <Icon name="chevron-right" size={14} /></span>{/if}
			</span>
		</Plank>
	</div>
</svelte:element>

<style>
	.shelfrow {
		--glow: var(--accent);
		--face: 112px;
		position: relative;
		display: block;
		padding-top: 30px;
		color: inherit;
		text-decoration: none;
		outline: none;
		animation: fill 520ms var(--spring) calc(var(--row) * 60ms) backwards;
	}

	@keyframes fill {
		from {
			opacity: 0;
			transform: translateY(14px);
		}
	}

	.bay {
		position: relative;
	}

	/* The shelf's own light, on when it is pointed at. */
	.bay::before {
		content: '';
		position: absolute;
		left: -2%;
		right: -2%;
		bottom: 0;
		height: 130%;
		background: radial-gradient(
			55% 70% at 35% 100%,
			color-mix(in srgb, var(--glow) 30%, transparent),
			transparent 70%
		);
		opacity: 0;
		transition: opacity 450ms var(--ease);
		pointer-events: none;
	}

	a.shelfrow:hover,
	a.shelfrow:focus-visible,
	:global([data-dragging]) > .shelfrow {
		--edge-light: 0.9;
		--spill: 1;
	}

	a.shelfrow:hover .bay::before,
	a.shelfrow:focus-visible .bay::before,
	:global([data-dragging]) > .shelfrow .bay::before {
		opacity: 1;
	}

	.stand {
		position: relative;
		display: flex;
		align-items: flex-end;
		gap: 12px;
		min-height: calc(var(--face) * 1.5);
		padding: 0 18px;
		perspective: 900px;
		transition: min-height 500ms var(--ease);
	}

	/* ---- a title face out ---- */
	.face {
		--rx: 0deg;
		--ry: 0deg;
		--lift: 0px;
		position: relative;
		flex: none;
		width: var(--face);
		aspect-ratio: 2 / 3;
		border-radius: 9px;
		background: var(--bg-elev-2);
		box-shadow:
			inset 0 1px 0 var(--shine),
			0 14px 24px -14px rgb(0 0 0 / 0.9);
		transform-origin: 50% 100%;
		transform: translateY(var(--lift)) rotateX(var(--rx)) rotateY(var(--ry));
		transition:
			transform 450ms var(--spring) calc(var(--k) * 22ms),
			width 500ms var(--ease);
		animation: place 680ms var(--spring) calc(var(--row) * 60ms + var(--k) * 30ms) backwards;
	}

	.face img,
	.face .blank {
		width: 100%;
		height: 100%;
		border-radius: inherit;
		object-fit: cover;
		display: block;
		user-select: none;
	}

	.face .blank {
		display: grid;
		place-items: center;
		padding: 8px;
		font-size: 0.72rem;
		text-align: center;
		color: var(--text-dim);
	}

	@keyframes place {
		from {
			opacity: 0;
			transform: translateY(-30px) rotate(-5deg) scale(0.96);
		}
	}

	a.shelfrow:hover .face {
		--lift: -7px;
	}

	.face:global(.tilt) {
		--lift: -16px !important;
		z-index: 3;
		transition-delay: 0ms;
		transition-duration: 200ms;
		box-shadow:
			inset 0 1px 0 var(--shine),
			0 28px 40px -14px rgb(0 0 0 / 0.95),
			0 0 0 1px color-mix(in srgb, var(--glow) 50%, transparent);
	}

	.peek {
		position: absolute;
		left: 50%;
		bottom: calc(100% + 8px);
		translate: -50% 0;
		z-index: 5;
		padding: 5px 10px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-float);
		color: var(--text);
		font-size: 0.78rem;
		font-weight: 600;
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transform: translateY(6px) scale(0.96);
		transition:
			opacity 180ms,
			transform 250ms var(--spring);
	}

	.peek .len {
		margin-left: 6px;
		color: var(--text-faint);
		font-weight: 500;
	}

	.face:global(.tilt) .peek,
	.spine:hover .peek {
		opacity: 1;
		transform: none;
	}

	/* ---- titles by their spines ---- */
	.books {
		display: flex;
		align-items: flex-end;
		gap: 3px;
		margin-left: 4px;
	}

	.spine {
		position: relative;
		flex: none;
		width: var(--sw);
		height: calc(var(--face) * 1.5 * var(--sh));
		border-radius: 3px 3px 2px 2px;
		background-color: var(--bg-elev-2);
		background-size: cover;
		background-position: center;
		transform-origin: 50% 100%;
		transition:
			transform 400ms var(--spring) calc(var(--k) * 14ms),
			filter 300ms,
			height 500ms var(--ease);
		animation: place 680ms var(--spring) calc(var(--row) * 60ms + var(--k) * 24ms) backwards;
	}

	/* The rounded back of a book: dark at the edges, a sheen down the middle,
	   and two bands across it. */
	.spine::before {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
		background:
			linear-gradient(
				180deg,
				transparent 9%,
				rgb(255 255 255 / 0.3) 9% calc(9% + 1px),
				transparent calc(9% + 1px) calc(88% - 1px),
				rgb(255 255 255 / 0.3) calc(88% - 1px) 88%,
				transparent 88%
			),
			linear-gradient(
				90deg,
				rgb(0 0 0 / 0.75),
				rgb(0 0 0 / 0.35) 22%,
				rgb(255 255 255 / 0.08) 45%,
				rgb(0 0 0 / 0.35) 70%,
				rgb(0 0 0 / 0.8)
			);
	}

	.spine .t {
		position: absolute;
		left: 50%;
		top: 13%;
		bottom: 15%;
		translate: -50% 0;
		overflow: hidden;
		writing-mode: vertical-rl;
		white-space: nowrap;
		text-overflow: ellipsis;
		color: rgb(255 255 255 / 0.92);
		text-shadow: 0 1px 2px rgb(0 0 0 / 0.9);
		font-size: 0.64rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		line-height: 1;
		text-transform: uppercase;
		transition: opacity 300ms;
	}

	.spine.lean {
		transform: rotate(-7deg) translateX(4px);
	}

	a.shelfrow:hover .spine {
		transform: translateY(-4px);
	}

	a.shelfrow:hover .spine.lean {
		transform: rotate(-3deg) translateY(-3px);
	}

	/* Pulled out a little, as you would to read it. */
	a.shelfrow .spine:hover {
		z-index: 3;
		transform: translateY(-18px);
		transition-delay: 0ms;
		filter: brightness(1.2);
	}

	/* ---- what did not fit, lying flat, and the bookend ---- */
	.pile {
		position: relative;
		flex: none;
		display: grid;
		justify-items: start;
		gap: 2px;
		margin-left: 10px;
		animation: place 680ms var(--spring) calc(var(--row) * 60ms + 400ms) backwards;
	}

	.pile i {
		display: block;
		height: 13px;
		border-radius: 2px;
		background-color: var(--bg-elev-2);
		background-size: cover;
		background-position: center;
		box-shadow:
			inset 0 3px 3px rgb(255 255 255 / 0.12),
			inset 0 -3px 3px rgb(0 0 0 / 0.6);
		filter: brightness(0.75);
		transition: transform 350ms var(--spring);
	}

	.pile i:nth-of-type(1) {
		width: 74px;
		transform: translateX(6px) rotate(-2deg);
	}

	.pile i:nth-of-type(2) {
		width: 84px;
	}

	.pile i:nth-of-type(3) {
		width: 80px;
		transform: translateX(3px);
	}

	a.shelfrow:hover .pile i:nth-of-type(1) {
		transform: translateX(10px) translateY(-2px) rotate(-5deg);
	}

	.more {
		position: absolute;
		left: 50%;
		bottom: calc(100% + 6px);
		translate: -50% 0;
		padding: 4px 9px;
		border: 1px solid var(--line);
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-size: 0.78rem;
		font-weight: 700;
		white-space: nowrap;
	}

	.vacant {
		align-self: center;
		padding: 0 6px;
		color: var(--text-faint);
		font-size: 0.86rem;
	}

	.bookend {
		position: relative;
		flex: none;
		width: 16px;
		height: calc(var(--face) * 1.05);
		margin-left: 6px;
		border-radius: 8px 8px 2px 2px;
		background: linear-gradient(90deg, var(--line-strong), var(--bg-elev) 45%, var(--line));
		box-shadow:
			inset 0 1px 0 var(--shine),
			0 10px 20px -8px rgb(0 0 0 / 0.8);
		transition:
			height 500ms var(--ease),
			box-shadow 400ms;
	}

	a.shelfrow:hover .bookend {
		box-shadow:
			inset 0 1px 0 var(--shine),
			0 0 18px color-mix(in srgb, var(--glow) 35%, transparent);
	}

	/* ---- the label on the shelf's edge ---- */
	.holder {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		padding: 6px 14px 6px 12px;
		border-radius: 9px;
		background: color-mix(in srgb, var(--bg) 35%, transparent);
		box-shadow:
			inset 0 1px 3px color-mix(in srgb, var(--bg) 70%, transparent),
			0 1px 0 var(--shine);
		transition: background 300ms var(--ease);
	}

	a.shelfrow:hover .holder,
	a.shelfrow:focus-visible .holder {
		background: color-mix(in srgb, var(--glow) 14%, transparent);
		box-shadow:
			inset 0 1px 3px color-mix(in srgb, var(--bg) 60%, transparent),
			0 0 0 1px color-mix(in srgb, var(--glow) 35%, transparent);
	}

	.name {
		min-width: 0;
		overflow: hidden;
		font-size: 1rem;
		font-weight: 750;
		letter-spacing: -0.01em;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.kind {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 8px;
		border-radius: var(--radius-pill);
		background: color-mix(in srgb, var(--glow) 14%, transparent);
		color: color-mix(in srgb, var(--glow) 70%, var(--text));
		font-size: 0.72rem;
		font-weight: 650;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.facts {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-left: auto;
		color: var(--text-faint);
		font-size: 0.84rem;
		white-space: nowrap;
	}

	.facts b {
		color: var(--text);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	.time {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--text-dim);
	}

	.ring {
		width: 22px;
		height: 22px;
		flex: none;
		transform: rotate(-90deg);
	}

	.ring circle {
		fill: none;
		stroke-width: 3.2;
	}

	.ring .track {
		stroke: var(--line-strong);
	}

	.ring .done {
		stroke: var(--glow);
		stroke-linecap: round;
	}

	.open {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		max-width: 0;
		overflow: hidden;
		opacity: 0;
		color: color-mix(in srgb, var(--glow) 70%, var(--text));
		font-weight: 650;
		transition:
			max-width 400ms var(--ease),
			opacity 250ms;
	}

	a.shelfrow:hover .open,
	a.shelfrow:focus-visible .open {
		max-width: 80px;
		opacity: 1;
	}

	/* ---- arranging: every shelf small enough that they all fit ---- */
	:global(.arranging) .shelfrow {
		--face: 62px;
		padding-top: 18px;
		animation: none;
	}

	:global(.arranging) .spine .t {
		opacity: 0;
	}

	@media (max-width: 720px) {
		.shelfrow {
			--face: 92px;
			padding-top: 14px;
		}

		.stand {
			overflow-x: auto;
			scrollbar-width: none;
			margin: 0 -14px;
			padding: 18px 26px 0;
			gap: 9px;
		}

		.stand::-webkit-scrollbar {
			display: none;
		}

		.facts {
			gap: 10px;
		}

		.count,
		.open {
			display: none;
		}

		:global(.arranging) .shelfrow {
			--face: 52px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.face,
		.spine,
		.pile,
		.shelfrow {
			animation: none;
		}
	}
</style>
