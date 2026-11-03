/*
 * Back closes what is open on top of the page — the command palette, a menu,
 * a dialog, a sheet — as Esc does on a computer, instead of leaving the page
 * under it.
 *
 * This is the browser's CloseWatcher: Android's Back, and Esc, close the
 * newest one open. It never touches the history, which matters: an earlier
 * version added a history step per layer and took it back on close, and that
 * raced the shallow navigations Library and Search use to keep their filters
 * in the address, so a Back taken at the wrong moment undid a filter. Where
 * there is no CloseWatcher (Safari, Firefox) Esc still closes everything.
 */

/**
 * Call from a component's script. While `isOpen()` is true, a close request
 * (Back on Android, Esc) runs `close`.
 */
export function backCloses(isOpen: () => boolean, close: () => void) {
	$effect(() => {
		if (!isOpen() || typeof CloseWatcher === 'undefined') return;
		let watcher: CloseWatcher;
		try {
			watcher = new CloseWatcher();
		} catch {
			return;
		}
		watcher.onclose = close;
		return () => watcher.destroy();
	});
}
