/*
 * Text cut off with an ellipsis or a line clamp shows in full on hover.
 *
 * One listener for the whole app rather than a `title` on every name: what
 * is cut off depends on the width it is drawn at, so it is measured when the
 * pointer arrives. A `title` the markup set itself is left alone.
 */

/** How far up from the element under the pointer to look for the clipped one. */
const DEPTH = 3;

function clipped(el: HTMLElement): boolean {
	const cs = getComputedStyle(el);
	if (cs.textOverflow === 'ellipsis' && el.scrollWidth > el.clientWidth + 1) return true;
	const clamp = cs.getPropertyValue('-webkit-line-clamp');
	return !!clamp && clamp !== 'none' && el.scrollHeight > el.clientHeight + 1;
}

function check(el: HTMLElement) {
	const ours = el.dataset.cutoff === '';
	if (el.hasAttribute('title') && !ours) return;
	const text = el.textContent?.replace(/\s+/g, ' ').trim();
	if (text && clipped(el)) {
		el.title = text;
		el.dataset.cutoff = '';
	} else if (ours) {
		el.removeAttribute('title');
		delete el.dataset.cutoff;
	}
}

/** Starts the listener; returns what stops it. */
export function showCutOffText(): () => void {
	const over = (e: PointerEvent) => {
		if (e.pointerType === 'touch') return;
		let el = e.target instanceof HTMLElement ? e.target : null;
		for (let i = 0; el && i < DEPTH; i++, el = el.parentElement) check(el);
	};
	document.addEventListener('pointerover', over, { passive: true });
	return () => document.removeEventListener('pointerover', over);
}
