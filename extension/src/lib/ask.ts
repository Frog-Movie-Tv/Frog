// The popup and the options page ask the background script; it answers
// `{ ok, value }` or `{ ok: false, error }`, which this turns back into a
// value or a thrown error.

import { browser } from 'wxt/browser';
import type { FromPopup } from './types.ts';

export async function ask<T>(msg: FromPopup): Promise<T> {
	const answer = (await browser.runtime.sendMessage(msg)) as
		{ ok: true; value: T } | { ok: false; error: string } | undefined;
	if (!answer) throw new Error('Frog’s extension is starting. Try again.');
	if (!answer.ok) throw new Error(answer.error);
	return answer.value;
}
