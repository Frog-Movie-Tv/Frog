/**
 * The first of each, by `key`, in order.
 *
 * A keyed `{#each}` throws on a repeated key, in production too, and takes the
 * whole page with it. Lists from TMDB and AniList do repeat, so anything drawn
 * from them goes through here on the way to the page. The server narrows its
 * own answers as well; this is what stands if one gets past it.
 */
export function uniqueBy<T>(list: readonly T[], key: (item: T) => unknown): T[] {
	const seen = new Set<unknown>();
	return list.filter((item) => {
		const k = key(item);
		if (seen.has(k)) return false;
		seen.add(k);
		return true;
	});
}
