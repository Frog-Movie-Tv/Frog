import { page } from '$app/state';

/**
 * The address the browser shows. A shallow navigation, which is how Search and
 * Library keep their filters in the address, leaves `page.url` on the route's
 * own address and puts this one in `page.shallow`, on the way there and when
 * Back returns to it.
 */
export function address(): typeof page.url {
	return page.shallow?.url ?? page.url;
}
