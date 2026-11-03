/**
 * Where a streaming logo goes. TMDB only knows which services carry a title,
 * not where on them, so each service we know opens its own search for the
 * title. A service whose search has no fixed address opens its home page.
 * Anything else goes through DuckDuckGo's first result for the service and the
 * title, which is nearly always the title's page on that service.
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
	[/youtube/i, (q) => `https://www.youtube.com/results?search_query=${q}`],
	[/rakuten viki|^viki/i, (q) => `https://www.viki.com/search?q=${q}`],
	[/rakuten/i, (q) => `https://www.rakuten.tv/be/search?q=${q}`],
	[/paramount/i, (q) => `https://www.paramountplus.com/search/?q=${q}`],
	[/vrt ?max/i, (q) => `https://www.vrt.be/vrtmax/zoeken/?q=${q}`],
	[/streamz/i, (q) => `https://www.streamz.be/streamz/zoeken?query=${q}`],
	[/goplay/i, (q) => `https://www.goplay.be/zoeken?q=${q}`],
	[/mubi/i, (q) => `https://mubi.com/en/search/films?query=${q}`],
	[/plex/i, (q) => `https://watch.plex.tv/search?q=${q}`],
	[/hidive/i, (q) => `https://www.hidive.com/search?q=${q}`],
	[/shudder/i, (q) => `https://www.shudder.com/search?q=${q}`],
	[/tubi/i, (q) => `https://tubitv.com/search/${q}`],
	[/pluto/i, (q) => `https://pluto.tv/search/details?query=${q}`],
	[/britbox/i, (q) => `https://www.britbox.com/search?q=${q}`],
	[/viaplay/i, (q) => `https://viaplay.com/search?query=${q}`],
	[/skyshowtime/i, () => 'https://www.skyshowtime.com/'],
	[/pickx/i, () => 'https://www.pickx.be/'],
	[/videoland/i, () => 'https://www.videoland.com/'],
	[/canal\+|canal plus/i, () => 'https://www.canalplus.com/'],
	[/animation digital network|^adn\b/i, () => 'https://animationdigitalnetwork.com/']
];

export function providerLink(provider: string, title: string): string {
	const q = encodeURIComponent(title);
	const found = SEARCH.find(([pattern]) => pattern.test(provider));
	if (found) return found[1](q);
	return `https://duckduckgo.com/?q=${encodeURIComponent(`! ${provider} ${title}`)}`;
}
