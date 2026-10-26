/**
 * Drag to reorder, for lists and grids alike.
 *
 * Pointer events rather than HTML5 drag and drop: the latter does nothing on
 * touch screens, and Frog is mostly used from a phone. Items carry
 * `data-sort-item`, and dragging starts only from a `data-sort-handle` inside
 * one, so taps and scrolling everywhere else behave normally. The handle also
 * answers the arrow keys, so ordering works without a pointer at all.
 *
 * The action never touches the DOM order itself. It reports moves; the
 * component reorders its array and a keyed `#each` moves the nodes.
 */
export interface SortableOptions {
	/** Move the item at `from` so it ends up at `to`. */
	onmove: (from: number, to: number) => void;
	/** The drag (or key press) is over: persist the order. */
	onend: () => void;
}

export function sortable(node: HTMLElement, options: SortableOptions) {
	let opts = options;
	let dragging: HTMLElement | null = null;
	let index = -1;

	const items = () => [...node.querySelectorAll<HTMLElement>('[data-sort-item]')];

	function down(event: PointerEvent) {
		const handle = (event.target as HTMLElement).closest<HTMLElement>('[data-sort-handle]');
		if (!handle || !node.contains(handle) || event.button > 0) return;
		const item = handle.closest<HTMLElement>('[data-sort-item]');
		if (!item) return;
		event.preventDefault();
		dragging = item;
		index = items().indexOf(item);
		item.dataset.dragging = '';
		node.dataset.sorting = '';
		// On the window, not the item: reordering moves the item's node, which
		// drops any pointer capture it had.
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		window.addEventListener('pointercancel', up);
	}

	function move(event: PointerEvent) {
		if (!dragging) return;
		const under = document
			.elementFromPoint(event.clientX, event.clientY)
			?.closest<HTMLElement>('[data-sort-item]');
		if (!under || under === dragging || !node.contains(under)) return;
		const to = items().indexOf(under);
		if (to < 0 || to === index) return;
		opts.onmove(index, to);
		index = to;
	}

	function up() {
		window.removeEventListener('pointermove', move);
		window.removeEventListener('pointerup', up);
		window.removeEventListener('pointercancel', up);
		if (!dragging) return;
		delete dragging.dataset.dragging;
		delete node.dataset.sorting;
		dragging = null;
		opts.onend();
	}

	function key(event: KeyboardEvent) {
		const handle = (event.target as HTMLElement).closest<HTMLElement>('[data-sort-handle]');
		if (!handle) return;
		const step = { ArrowUp: -1, ArrowLeft: -1, ArrowDown: 1, ArrowRight: 1 }[event.key];
		if (!step) return;
		const list = items();
		const from = list.indexOf(handle.closest<HTMLElement>('[data-sort-item]')!);
		const to = from + step;
		if (from < 0 || to < 0 || to >= list.length) return;
		event.preventDefault();
		opts.onmove(from, to);
		opts.onend();
		// The keyed list moves the node; keep focus on the same handle.
		queueMicrotask(() => handle.focus());
	}

	node.addEventListener('pointerdown', down);
	node.addEventListener('keydown', key);

	return {
		update(next: SortableOptions) {
			opts = next;
		},
		destroy() {
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('keydown', key);
			up();
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
