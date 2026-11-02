/**
 * Getting a picture ready to upload. All of it happens here, in the browser, so
 * the server never decodes an image: it has 954 MB and checks only the size and
 * the first few bytes. Pictures already uploaded stay as they are.
 */

/** The server refuses anything larger; saying so here saves sending it. */
const MAX_BYTES = 3 * 1024 * 1024;

/** A profile picture is never drawn past 200 pixels, so this is sharp at 2.5×. */
export const AVATAR_PIXELS = 512;
/** The longest side of a banner: full width on a large screen. */
export const BANNER_PIXELS = 1800;

const QUALITY = 0.86;
const TOO_LARGE = 'That picture is too large; keep it under 3 MB.';
const UNREADABLE = 'That picture could not be read. Try a JPEG or PNG.';

/** JPEG has no transparency, so only a JPEG stays one. */
export const outputType = (source: string) => (source === 'image/jpeg' ? source : 'image/webp');

/** A moving GIF cannot go through a canvas without stopping. */
export const staysAsItIs = (source: string) => source === 'image/gif';

const toBlob = (canvas: HTMLCanvasElement, type: string) =>
	new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, type, QUALITY));

/**
 * What is on a canvas, as a file the server accepts. A browser with no WebP
 * writer gives back a PNG, which can run large, so JPEG is the way out of that.
 */
export async function encode(canvas: HTMLCanvasElement, source: string): Promise<Blob> {
	let blob = await toBlob(canvas, outputType(source));
	if (blob && blob.size > MAX_BYTES) blob = await toBlob(canvas, 'image/jpeg');
	if (!blob) throw new Error(UNREADABLE);
	if (blob.size > MAX_BYTES) throw new Error(TOO_LARGE);
	return blob;
}

/** The size of a picture in pixels, and a bitmap to draw it from. */
export async function decode(file: Blob): Promise<ImageBitmap> {
	try {
		return await createImageBitmap(file);
	} catch {
		throw new Error(UNREADABLE);
	}
}

/** Scale a picture so its longest side is at most `longest` pixels. */
export async function shrink(file: File, longest: number): Promise<Blob> {
	if (staysAsItIs(file.type)) {
		if (file.size > MAX_BYTES) throw new Error(TOO_LARGE);
		return file;
	}
	const bitmap = await decode(file);
	const scale = Math.min(1, longest / Math.max(bitmap.width, bitmap.height));
	const canvas = document.createElement('canvas');
	canvas.width = Math.round(bitmap.width * scale);
	canvas.height = Math.round(bitmap.height * scale);
	canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
	bitmap.close();
	return encode(canvas, file.type);
}
