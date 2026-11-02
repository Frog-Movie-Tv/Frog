declare global {
	namespace App {
		interface PageState {
			/** How many layers are open over the page (`lib/layers.svelte.ts`). */
			layer?: number;
		}
	}
}

export {};
