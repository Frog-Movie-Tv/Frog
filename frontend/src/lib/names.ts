/** The server holds a new name to this too. Never applied when signing in. */
export const USERNAME_MAX = 20;

const words = typeof Intl.Segmenter === 'function' ? new Intl.Segmenter() : null;

/** Characters as a person counts them: a flag or a family emoji is one. */
export function characters(text: string): number {
	return words ? [...words.segment(text)].length : [...text].length;
}

/**
 * What a name field's `maxlength` should be right now. The browser counts in
 * UTF-16 units, where one family emoji takes eleven, so the limit moves with what
 * is typed and the field stops at the same place the server does.
 */
export function nameLimit(value: string): number {
	return USERNAME_MAX + value.length - characters(value);
}
