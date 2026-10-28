/**
 * Where a streaming logo goes. TMDB only knows which services carry a title,
 * not where on them, so each service we know opens its own search for the
 * title; anything else falls back to TMDB's watch page for it.
 *
 * Matched by name rather than id: TMDB gives the same service different ids
 * in different regions and for its ad-supported or channel variants.
 */
const SEARCH: [RegExp, (q: string) => string][] = [
	[/netflix/i, (q) => `https://www.netflix.com/search?q=${q}`],
	[/prime video|amazon video/i, (q) => `https://www.primevideo.com/search?phrase=${q}`],
	[/disney/i, (q) => `https://www.disneyplus.com/search?q=${q}`],
	[/apple tv/i, (q) => `https://tv.apple.com/search?term=${q}`],
	[/^(hbo )?max\b|hbo max/i, (q) => `https://play.hbomax.com/search?q=${q}`],
	[/crunchyroll/i, (q) => `https://www.crunchyroll.com/search?q=${q}`],
	[/hulu/i, (q) => `https://www.hulu.com/search?q=${q}`],
	[/peacock/i, (q) => `https://www.peacocktv.com/search?q=${q}`],
	[/google play/i, (q) => `https://play.google.com/store/search?q=${q}&c=movies`],
	[/youtube/i, (q) => `https://www.youtube.com/results?search_query=${q}`]
];

export function providerLink(
	provider: string,
	title: string,
	fallback: string | null
): string | null {
	const q = encodeURIComponent(title);
	const found = SEARCH.find(([pattern]) => pattern.test(provider));
	return found ? found[1](q) : fallback;
}
