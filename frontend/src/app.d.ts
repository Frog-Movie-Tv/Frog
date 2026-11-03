declare global {
	/**
	 * Back on Android, and Esc, as a request to close what is on top
	 * (`lib/layers.svelte.ts`). Chromium has it; TypeScript's DOM types do not yet.
	 */
	class CloseWatcher extends EventTarget {
		constructor(options?: { signal?: AbortSignal });
		onclose: ((event: Event) => void) | null;
		requestClose(): void;
		close(): void;
		destroy(): void;
	}
}

export {};
