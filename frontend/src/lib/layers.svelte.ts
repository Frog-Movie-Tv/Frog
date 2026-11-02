import { goto } from '$app/navigation';
import { navigating, page } from '$app/state';
import { address } from './address';

/*
 * Back closes what is open on top of the page — the command palette, a menu,
 * a dialog, a sheet — as Esc does on a computer, instead of leaving the page
 * under it. Opening one adds a step to the history: the same address, with
 * the layer named in `page.state` (a shallow navigation, so nothing reloads).
 * Back takes that step away and the top layer closes. Closed any other way,
 * its step is taken back out, so the next Back leaves the page as it should.
 */

interface Layer {
	close: () => void;
}

const layers: Layer[] = [];
/** Steps this module took back itself, whose popstate is not a Back press. */
let ownBacks = 0;
let listening = false;

function listen() {
	if (listening || typeof window === 'undefined') return;
	listening = true;
	window.addEventListener('popstate', () => {
		if (ownBacks > 0) {
			ownBacks--;
			return;
		}
		layers.pop()?.close();
	});
}

/**
 * Call from a component's script. While `isOpen()` is true the layer holds a
 * step in the history; Back runs `close`.
 */
export function backCloses(isOpen: () => boolean, close: () => void) {
	$effect(() => {
		if (!isOpen()) return;
		listen();
		const layer: Layer = { close };
		layers.push(layer);
		const depth = layers.length;
		void goto(address().href, {
			shallow: true,
			state: { ...page.state, layer: depth }
		});
		return () => {
			const at = layers.indexOf(layer);
			// Already taken away by Back: nothing more to do.
			if (at === -1) return;
			layers.splice(at, 1);
			// Closed by a choice that goes somewhere else: that navigation
			// replaces the page, and the step is left behind it, harmless.
			// So is a step something else has since replaced.
			setTimeout(() => {
				if (navigating.to || page.state.layer !== depth) return;
				ownBacks++;
				history.back();
			}, 0);
		};
	});
}
