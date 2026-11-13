/** Phones draw menus as sheets from the bottom instead; see each menu's CSS. */
export const SHEET_QUERY = '(max-width: 600px)';

/**
 * Keeps a menu inside the window: moved sideways when it would run off an
 * edge, and opened above its button when there is no room below. Use on the
 * element that drops down, as `use:onScreen`; `use:onScreen={false}` for one
 * that stays a dropdown on a phone as well.
 */
export function onScreen(node: HTMLElement, sheet = true) {
	const fit = () => {
		node.style.translate = '';
		node.style.top = '';
		node.style.bottom = '';
		// Drawn as a sheet on a phone: nothing to nudge.
		if (sheet && window.matchMedia(SHEET_QUERY).matches) return;
		const pad = 8;
		const r = node.getBoundingClientRect();
		let dx = 0;
		if (r.right > window.innerWidth - pad) dx = window.innerWidth - pad - r.right;
		if (r.left + dx < pad) dx = pad - r.left;
		if (dx) node.style.translate = `${Math.round(dx)}px 0`;
		const roomAbove = r.top - r.height - (node.offsetParent?.getBoundingClientRect().height ?? 0);
		if (r.bottom > window.innerHeight - pad && roomAbove > pad) {
			node.style.top = 'auto';
			node.style.bottom = 'calc(100% + 6px)';
		}
	};
	const frame = requestAnimationFrame(fit);
	window.addEventListener('resize', fit);
	return {
		destroy() {
			cancelAnimationFrame(frame);
			window.removeEventListener('resize', fit);
		}
	};
}

/**
 * Places a `position: fixed` element under its button, right edges lined up,
 * inside the window, and above the button when there is no room below. When
 * it fits on neither side it takes the roomier one and shrinks to it, so a
 * long list scrolls inside the window instead of running off it. It follows
 * the button through scrolling, and is never narrower than the button. For a
 * panel moved out of its container with `portal`, since a pane with
 * `overflow: hidden` cuts off anything drawn inside it; on a phone it is left
 * to its sheet CSS.
 */
export function anchorTo(node: HTMLElement, anchor: HTMLElement | undefined) {
	const pad = 8;
	const gap = 8;
	const place = () => {
		node.style.maxHeight = '';
		if (!anchor || window.matchMedia(SHEET_QUERY).matches) {
			node.style.left = '';
			node.style.top = '';
			node.style.minWidth = '';
			return;
		}
		const a = anchor.getBoundingClientRect();
		node.style.minWidth = `${Math.round(a.width)}px`;
		const r = node.getBoundingClientRect();
		const left = Math.min(
			Math.max(pad, a.right - r.width),
			Math.max(pad, window.innerWidth - pad - r.width)
		);
		const below = window.innerHeight - pad - (a.bottom + gap);
		const above = a.top - gap - pad;
		let height = r.height;
		let top = a.bottom + gap;
		if (height > below) {
			if (height <= above) {
				top = a.top - gap - height;
			} else if (above > below) {
				height = above;
				top = pad;
			} else {
				height = below;
			}
			if (height < r.height) node.style.maxHeight = `${Math.floor(height)}px`;
		}
		node.style.left = `${Math.round(left)}px`;
		node.style.top = `${Math.round(top)}px`;
	};
	place();
	const frame = requestAnimationFrame(place);
	window.addEventListener('resize', place);
	window.addEventListener('scroll', place, true);
	return {
		update(next: HTMLElement | undefined) {
			anchor = next;
			place();
		},
		destroy() {
			cancelAnimationFrame(frame);
			window.removeEventListener('resize', place);
			window.removeEventListener('scroll', place, true);
		}
	};
}
