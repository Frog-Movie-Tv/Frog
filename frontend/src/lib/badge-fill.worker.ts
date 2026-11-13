/// <reference lib="webworker" />
/**
 * The badge editor's background fill, off the main thread, so the sliders
 * stay smooth on a phone. Each message is one picture; the answer hands the
 * pixels back without copying them.
 */
import { removeBackground, type FillOptions } from './badge-fill';

interface Job {
	id: number;
	data: Uint8ClampedArray<ArrayBuffer>;
	width: number;
	height: number;
	options: FillOptions;
}

const scope = self as unknown as DedicatedWorkerGlobalScope;

scope.onmessage = (event: MessageEvent<Job>) => {
	const { id, data, width, height, options } = event.data;
	const seed = removeBackground(data, width, height, options);
	scope.postMessage({ id, data, seed }, [data.buffer]);
};
