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
