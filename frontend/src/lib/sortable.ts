/**
 * Drag to reorder, for lists, grids and rows of chips alike.
 *
 * Pointer events rather than HTML5 drag and drop: the latter does nothing on
 * touch screens, and Frog is mostly used from a phone. Items carry
 * `data-sort-item`, and dragging starts only from a `data-sort-handle` inside
 * one, so taps and scrolling everywhere else behave normally. The handle also
 * answers the arrow keys, so ordering works without a pointer at all.
 *
 * The lifted item follows the pointer exactly, and the others slide aside to
 * open a gap where it would land. The action never touches the DOM order
 * itself: it reports moves, the component reorders its array, a keyed
 * `#each` moves the nodes, and every item that moved is animated from where
 * it was drawn to where it now sits (first, last, invert, play), so any
 * layout the browser can wrap works without measuring it here. Letting go
 * settles the item into its gap; Escape puts it back where it started.
 */
import { tick } from 'svelte';

export interface SortableOptions {
	/** Move the item at `from` so it ends up at `to`. */
	onmove: (from: number, to: number) => void;
	/** The drag (or key press) is over, put back or not: persist the order. */
	onend: () => void;
}

/** Pixels the pointer travels before a press on a handle becomes a drag. */
const THRESHOLD = 4;
/** How close to the window's top or bottom edge the page starts scrolling. */
const EDGE = 56;
const SLIDE_MS = 200;
const EASE = 'cubic-bezier(0.2, 0.8, 0.2, 1)';

const still = () =>
	typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Where an element sits by layout alone, in window coordinates: what a
 *  running slide or the drag's own transform would hide. */
function laidOut(el: HTMLElement): DOMRect {
	const parent = el.offsetParent as HTMLElement | null;
	if (!parent) return el.getBoundingClientRect();
	const p = parent.getBoundingClientRect();
	return new DOMRect(
		p.left + parent.clientLeft + el.offsetLeft - parent.scrollLeft,
		p.top + parent.clientTop + el.offsetTop - parent.scrollTop,
		el.offsetWidth,
		el.offsetHeight
	);
}

export function sortable(node: HTMLElement, options: SortableOptions) {
	let opts = options;
	/** The item under the pointer since it was pressed, lifted or not. */
	let item: HTMLElement | null = null;
	let lifted = false;
	let start = -1;
	let index = -1;
	let pointer = { x: 0, y: 0 };
	let pressed = { x: 0, y: 0 };
	/** Where in the item it was picked up, so it does not jump to the pointer. */
	let grab = { x: 0, y: 0 };
	let frame = 0;
	/** A move on its way into the DOM; the next waits for it. */
	let reordering: Promise<void> | null = null;
	/** The click a drag ends with, which must not press what lies under it. */
	let swallowNext = false;

	const items = () => [...node.querySelectorAll<HTMLElement>('[data-sort-item]')];

	function follow() {
		if (!item || !lifted) return;
		const at = laidOut(item);
		const x = pointer.x - grab.x - at.left;
		const y = pointer.y - grab.y - at.top;
		// A small card grows a little as it lifts; a wide row would only be
		// cut off by the pane it sits in, so it lifts by its shadow alone.
		const grow = at.width > 320 ? '' : ' scale(1.04)';
		item.style.transform = `translate(${x}px, ${y}px)${grow}`;
	}

	/** Move the lifted item to `to`, sliding every other item that moves. */
	function reorder(to: number): Promise<void> {
		const run = slide(to);
		reordering = run;
		void run.finally(() => {
			if (reordering === run) reordering = null;
		});
		return run;
	}

	async function slide(to: number) {
		const list = items();
		const before = new Map(list.map((el) => [el, el.getBoundingClientRect()]));
		opts.onmove(index, to);
		index = to;
		await tick();
		follow();
		if (still()) return;
		for (const el of items()) {
			if (el === item) continue;
			const was = before.get(el);
			if (!was) continue;
			// Measured with any slide still running, so a slide interrupted by
			// the next one carries on from where it was drawn.
			for (const a of el.getAnimations()) a.cancel();
			const now = el.getBoundingClientRect();
			const dx = was.left - now.left;
			const dy = was.top - now.top;
			if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
			el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], {
				duration: SLIDE_MS,
				easing: EASE
			});
		}
	}

	/** Which slot the pointer asks for: the item under it, once the pointer
	 *  is past that item's middle in the direction of travel, so two items of
	 *  different sizes do not trade places back and forth. */
	function wanted(): number {
		if (!item) return index;
		const list = items();
		for (let i = 0; i < list.length; i++) {
			const el = list[i];
			if (el === item) continue;
			const r = laidOut(el);
			if (pointer.x < r.left || pointer.x > r.right || pointer.y < r.top || pointer.y > r.bottom) {
				continue;
			}
			const own = laidOut(item);
			const sameRow = own.top < r.bottom && r.top < own.bottom;
			const past = sameRow
				? i > index
					? pointer.x > r.left + r.width / 2
					: pointer.x < r.left + r.width / 2
				: i > index
					? pointer.y > r.top + r.height / 2
					: pointer.y < r.top + r.height / 2;
			return past ? i : index;
		}
		return index;
	}

	function step() {
		frame = 0;
		if (!item || !lifted) return;
		// Near the window's edge the page scrolls, so a long list can be
		// arranged without letting go.
		const h = window.innerHeight;
		const speed =
			pointer.y < EDGE
				? -Math.ceil((EDGE - pointer.y) / 4)
				: pointer.y > h - EDGE
					? Math.ceil((pointer.y - (h - EDGE)) / 4)
					: 0;
		if (speed) window.scrollBy(0, speed);
		if (!reordering) {
			const to = wanted();
			if (to !== index) void reorder(to);
		}
		follow();
		if (speed) frame = requestAnimationFrame(step);
	}

	function schedule() {
		if (!frame) frame = requestAnimationFrame(step);
	}

	function lift() {
		if (!item) return;
		lifted = true;
		const at = item.getBoundingClientRect();
		grab = { x: pressed.x - at.left, y: pressed.y - at.top };
		item.dataset.dragging = '';
		node.dataset.sorting = '';
		follow();
	}

	function down(event: PointerEvent) {
		const handle = (event.target as HTMLElement).closest<HTMLElement>('[data-sort-handle]');
		if (!handle || !node.contains(handle) || event.button > 0) return;
		const it = handle.closest<HTMLElement>('[data-sort-item]');
		if (!it) return;
		// No text selection or native image drag while pressing.
		event.preventDefault();
		item = it;
		lifted = false;
		start = index = items().indexOf(it);
		pointer = pressed = { x: event.clientX, y: event.clientY };
		// On the window, not the item: reordering moves the item's node, which
		// drops any pointer capture it had.
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		window.addEventListener('pointercancel', cancel);
		window.addEventListener('keydown', escape, true);
		window.addEventListener('scroll', schedule, true);
	}

	function move(event: PointerEvent) {
		if (!item) return;
		pointer = { x: event.clientX, y: event.clientY };
		if (!lifted) {
			if (Math.hypot(pointer.x - pressed.x, pointer.y - pressed.y) < THRESHOLD) return;
			lift();
		}
		event.preventDefault();
		schedule();
	}

	function stopListening() {
		window.removeEventListener('pointermove', move);
		window.removeEventListener('pointerup', up);
		window.removeEventListener('pointercancel', cancel);
		window.removeEventListener('keydown', escape, true);
		window.removeEventListener('scroll', schedule, true);
		if (frame) cancelAnimationFrame(frame);
		frame = 0;
	}

	/** Let go: the item glides from under the pointer into its gap. */
	function settle(el: HTMLElement) {
		const from = el.style.transform;
		el.style.transform = '';
		delete el.dataset.dragging;
		delete node.dataset.sorting;
		if (!from || still()) return;
		el.dataset.settling = '';
		const glide = el.animate([{ transform: from }, { transform: 'none' }], {
			duration: SLIDE_MS,
			easing: EASE
		});
		const done = () => delete el.dataset.settling;
		glide.onfinish = done;
		glide.oncancel = done;
	}

	async function up(event?: PointerEvent) {
		stopListening();
		if (event && lifted) pointer = { x: event.clientX, y: event.clientY };
		const el = item;
		const wasLifted = lifted;
		// A quick flick can let go before the next frame looked where it was.
		if (el && wasLifted) {
			while (reordering) await reordering;
			const to = wanted();
			if (to !== index) await reorder(to);
		}
		item = null;
		lifted = false;
		if (!el || !wasLifted) return;
		swallowNext = true;
		setTimeout(() => (swallowNext = false));
		settle(el);
		opts.onend();
	}

	/** Escape, or the browser taking the pointer away: back where it was. */
	async function cancel() {
		stopListening();
		const el = item;
		const wasLifted = lifted;
		while (reordering) await reordering;
		if (el && wasLifted && index !== start) await reorder(start);
		item = null;
		lifted = false;
		if (el && wasLifted) {
			settle(el);
			opts.onend();
		}
	}

	function escape(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !item) return;
		event.preventDefault();
		event.stopPropagation();
		void cancel();
	}

	function key(event: KeyboardEvent) {
		if (item) return;
		const handle = (event.target as HTMLElement).closest<HTMLElement>('[data-sort-handle]');
		if (!handle) return;
		const delta = { ArrowUp: -1, ArrowLeft: -1, ArrowDown: 1, ArrowRight: 1 }[event.key];
		if (!delta) return;
		const list = items();
		const el = handle.closest<HTMLElement>('[data-sort-item]');
		const from = el ? list.indexOf(el) : -1;
		const to = from + delta;
		if (from < 0 || to < 0 || to >= list.length) return;
		event.preventDefault();
		index = from;
		item = el;
		void reorder(to).then(() => {
			item = null;
			opts.onend();
			// The keyed list moved the node; keep focus on the same handle.
			handle.focus();
		});
	}

	/** A click that ends a drag is not a press of whatever lies under it. */
	function swallow(event: MouseEvent) {
		if (!swallowNext) return;
		swallowNext = false;
		event.preventDefault();
		event.stopPropagation();
	}

	node.addEventListener('pointerdown', down);
	node.addEventListener('keydown', key);
	node.addEventListener('click', swallow, true);

	return {
		update(next: SortableOptions) {
			opts = next;
		},
		destroy() {
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('keydown', key);
			node.removeEventListener('click', swallow, true);
			stopListening();
			if (item) {
				item.style.transform = '';
				delete item.dataset.dragging;
			}
			delete node.dataset.sorting;
		}
	};
}

/** Return a copy of `list` with one item moved. */
export function moved<T>(list: T[], from: number, to: number): T[] {
	const next = [...list];
	const [item] = next.splice(from, 1);
	next.splice(to, 0, item);
	return next;
}
