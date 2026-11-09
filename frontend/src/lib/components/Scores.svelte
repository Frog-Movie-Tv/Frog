<script lang="ts">
	import { ANILIST_MARK } from '#lib/marks';
	import type { TitleScore } from '#lib/types';

	interface Props {
		scores: TitleScore[];
	}

	let { scores }: Props = $props();

	/** The sites a title page shows, in this order. Scores arrive 0–100 and
	 *  are shown that way, as a percentage, whatever scale the site uses. */
	const MAIN = ['anilist', 'imdb', 'tomatoes', 'metacritic', 'letterboxd', 'myanimelist'] as const;
	/** Read through MDBList, which has a daily limit. */
	const LIMITED = ['imdb', 'tomatoes', 'metacritic', 'letterboxd', 'myanimelist'];

	const NAMES: Record<string, string> = {
		anilist: 'AniList',
		imdb: 'IMDb',
		tomatoes: 'Rotten Tomatoes',
		metacritic: 'Metacritic',
		letterboxd: 'Letterboxd',
		myanimelist: 'MyAnimeList',
		tmdb: 'TMDB'
	};

	/*
	 * The sites' own marks, from Simple Icons (CC0), on a 24-unit square.
	 * Each is drawn the way the site shows it: its colour, its tile.
	 */
	const IMDB =
		'M22.3781 0H1.6218C.7411.0583.0587.7437.0018 1.5953l-.001 20.783c.0585.8761.7125 1.543 1.5559 1.6191A.337.337 0 0 0 1.6016 24h20.7971a.4579.4579 0 0 0 .0437-.002c.8727-.0768 1.5568-.8271 1.5568-1.7085V1.7098c0-.8914-.696-1.6416-1.584-1.7078A.3294.3294 0 0 0 22.3781 0zm0 .496a1.2144 1.2144 0 0 1 1.1252 1.2139v20.5797c0 .6377-.4875 1.1602-1.1045 1.2145H1.6016c-.5967-.0543-1.0645-.5297-1.1053-1.1258V1.6284C.5371 1.0185 1.0184.5364 1.6217.496h20.7564zM4.7954 8.2603v7.3636H2.8899V8.2603h1.9055zm6.5367 0v7.3636H9.6707v-4.9704l-.6711 4.9704H7.813l-.6986-4.8618-.0066 4.8618h-1.668V8.2603h2.468c.0748.4476.1492.9694.2307 1.5734l.2712 1.8713.4407-3.4447h2.4817zm2.9772 1.3289c.0742.0404.122.108.1417.2034.0279.0953.0345.3118.0345.6442v2.8548c0 .4881-.0345.7867-.0955.8954-.0609.1152-.2304.1695-.5018.1695V9.5211c.204 0 .3457.0205.4211.0681zm-.0211 6.0347c.4543 0 .8006-.0265 1.0245-.0742.2304-.0477.4204-.1357.5694-.2648.1556-.1218.2642-.298.3251-.5219.0611-.2238.1021-.6648.1021-1.3224v-2.5832c0-.6986-.0271-1.1668-.0742-1.4039-.041-.237-.1431-.4543-.3126-.6437-.1695-.1973-.4198-.3324-.7456-.421-.3191-.0808-.8542-.1285-1.7694-.1285h-1.4244v7.3636h2.3051zm5.14-1.7827c0 .3523-.0199.5762-.0544.6708-.033.0947-.1894.1424-.3046.1424-.1086 0-.19-.0477-.2238-.1351-.041-.0887-.0609-.2986-.0609-.6238v-1.9469c0-.3324.0199-.5423.0543-.6237.0338-.0808.1086-.122.2171-.122.1153 0 .2709.0412.3114.1425.041.0947.0609.2986.0609.6032v1.8926zm-2.4747-5.5809v7.3636h1.7157l.1152-.4675c.1556.1894.3251.3324.5152.4271.1828.0881.4608.1357.678.1357.3047 0 .5629-.0748.7802-.237.2165-.1562.3589-.3462.4198-.5628.0543-.2173.0887-.543.0887-.9841v-2.0675c0-.4409-.0139-.7324-.0344-.8681-.0199-.1357-.0742-.2781-.1695-.4204-.1021-.1425-.2437-.251-.4272-.3325-.1834-.0742-.3999-.1152-.6576-.1152-.2172 0-.4952.0477-.6846.1285-.1835.0887-.353.2238-.5086.4007V8.2603h-1.8309z';
	const TOMATO =
		'M5.866 0L4.335 1.262l2.082 1.8c-2.629-.989-4.842 1.4-5.012 2.338 1.384-.323 2.24-.422 3.344-.335-7.042 4.634-4.978 13.148-1.434 16.094 5.784 4.612 13.77 3.202 17.91-1.316C27.26 13.363 22.993.65 10.86 2.766c.107-1.17.633-1.503 1.243-1.602-.89-1.493-3.67-.734-4.556 1.374C7.52 2.602 5.866 0 5.866 0zM4.422 7.217H6.9c2.673 0 2.898.012 3.55.202 1.06.307 1.868.973 2.313 1.904.05.106.092.206.13.305l7.623.008.027 2.912-2.745-.024v7.549l-2.982-.016v-7.522l-2.127.016a2.92 2.92 0 0 1-1.056 1.134c-.287.176-.3.19-.254.264.127.2 2.125 3.642 2.125 3.659l-3.39.019-2.013-3.376c-.034-.047-.122-.068-.344-.084l-.297-.02.037 3.48-3.075-.038zm3.016 2.288l.024.338c.014.186.024.729.024 1.206v.867l.582-.025c.32-.013.695-.049.833-.078.694-.146 1.048-.478 1.087-1.018.027-.378-.063-.636-.303-.87-.318-.309-.761-.416-1.733-.418Z';
	const METACRITIC =
		'M11.99 0A12 12 0 1 0 24 12v-.014A12 12 0 0 0 11.99 0Zm-.055 2.564a9.399 9.399 0 0 1 9.407 9.389v.01a9.399 9.399 0 1 1-9.408-9.399Zm-1.61 17.198 2.046-2.046-3.94-3.94c-.165-.166-.345-.373-.442-.608-.221-.47-.318-1.203.221-1.742.664-.664 1.548-.387 2.406.47l3.788 3.788 2.046-2.046-3.954-3.954a2.48 2.48 0 0 1-.456-.622c-.263-.539-.25-1.216.235-1.7.677-.678 1.562-.429 2.544.553l3.677 3.677 2.046-2.046-3.982-3.982c-2.018-2.018-3.912-1.949-5.212-.65-.498.499-.802 1.024-.954 1.618a4.026 4.026 0 0 0-.055 1.686l-.027.028c-.996-.414-2.13-.166-3 .705-1.162 1.161-1.12 2.392-.982 3.11l-.042.043-1.009-.816-1.77 1.77a64.1 64.1 0 0 1 2.213 2.1z';
	const ANILIST = ANILIST_MARK;
	const MAL =
		'M14.921 6.479c-.82 0-3.683 0-4.947 3.156-.662 1.652-.986 4.812.876 7.886l1.934-1.41s-.767-1.095-1.083-3.191h2.897l.022 3.19h2.604V8.835h-2.581v2.043l-2.46-.023s.413-2.408 2.877-2.336h2.454l-.572-2.04ZM0 6.528v9.624h2.348v-5.84l2.031 2.664 2.047-2.652v5.828h2.336V6.528H6.437L4.368 9.474 2.31 6.528Zm18.447.022v9.583h5.022L24 14.09h-3.232V6.55Z';
	const TMDB =
		'M6.62 12a2.291 2.291 0 0 1 2.292-2.295h-.013A2.291 2.291 0 0 1 11.189 12a2.291 2.291 0 0 1-2.29 2.291h.013A2.291 2.291 0 0 1 6.62 12zm10.72-4.062h4.266a2.291 2.291 0 0 0 2.29-2.291 2.291 2.291 0 0 0-2.29-2.296H17.34a2.291 2.291 0 0 0-2.291 2.296 2.291 2.291 0 0 0 2.29 2.29zM2.688 20.645h8.285a2.291 2.291 0 0 0 2.291-2.292 2.291 2.291 0 0 0-2.29-2.295H2.687a2.291 2.291 0 0 0-2.291 2.295 2.291 2.291 0 0 0 2.29 2.292zm10.881-6.354h.81l1.894-4.586H15.19l-1.154 3.008h-.013l-1.135-3.008h-1.154zm4.208 0h1.011V9.705h-1.011zm2.878 0h3.235v-.93h-2.223v-.933h1.99v-.934h-1.99v-.855h2.107v-.934h-3.112zM1.31 7.941h1.01V4.247h1.31v-.895H0v.895h1.31zm3.747 0h1.011V5.959h1.958v1.984h1.011v-4.59h-1.01v1.711H6.061V3.351H5.057zm5.348 0h3.242v-.933H11.41v-.934h1.99v-.933h-1.99v-.856h2.107v-.934h-3.112zM.162 14.296h1.005v-3.52h.013l1.167 3.52h.765l1.206-3.52h.013v3.52h1.011v-4.59H3.82L2.755 12.7h-.013L1.686 9.705H.156zm14.534 6.353h1.641a3.188 3.188 0 0 0 .98-.149 2.531 2.531 0 0 0 .824-.437 2.123 2.123 0 0 0 .567-.713 2.193 2.193 0 0 0 .223-.983 2.399 2.399 0 0 0-.218-1.07 1.958 1.958 0 0 0-.586-.716 2.405 2.405 0 0 0-.873-.392 4.349 4.349 0 0 0-1.046-.13h-1.519zm1.013-3.656h.596a2.26 2.26 0 0 1 .606.08 1.514 1.514 0 0 1 .503.244 1.167 1.167 0 0 1 .34.412 1.28 1.28 0 0 1 .13.587 1.546 1.546 0 0 1-.13.658 1.127 1.127 0 0 1-.347.433 1.41 1.41 0 0 1-.518.238 2.797 2.797 0 0 1-.649.07h-.538zm4.686 3.656h1.88a2.997 2.997 0 0 0 .613-.064 1.735 1.735 0 0 0 .554-.214 1.221 1.221 0 0 0 .402-.39 1.105 1.105 0 0 0 .155-.606 1.188 1.188 0 0 0-.071-.415 1.01 1.01 0 0 0-.204-.34 1.087 1.087 0 0 0-.317-.24 1.297 1.297 0 0 0-.413-.13v-.012a1.203 1.203 0 0 0 .575-.366.962.962 0 0 0 .216-.648 1.081 1.081 0 0 0-.149-.603 1.022 1.022 0 0 0-.389-.354 1.673 1.673 0 0 0-.54-.169 4.463 4.463 0 0 0-.6-.041h-1.712zm1.011-3.734h.687a1.4 1.4 0 0 1 .24.022.748.748 0 0 1 .22.075.432.432 0 0 1 .16.147.418.418 0 0 1 .061.236.47.47 0 0 1-.055.233.433.433 0 0 1-.146.156.62.62 0 0 1-.204.084 1.058 1.058 0 0 1-.23.026h-.745zm0 1.835h.765a1.96 1.96 0 0 1 .266.02 1.015 1.015 0 0 1 .26.07.519.519 0 0 1 .204.152.406.406 0 0 1 .08.26.481.481 0 0 1-.06.253.519.519 0 0 1-.16.168.62.62 0 0 1-.217.09 1.155 1.155 0 0 1-.237.027H21.4z';
	/** Letterboxd's three dots, each in its own colour. */
	const LETTERBOXD = [
		{
			d: 'M8.224 14.352a4.447 4.447 0 0 1-3.775 2.092C1.992 16.444 0 14.454 0 12s1.992-4.444 4.45-4.444c1.592 0 2.988.836 3.774 2.092-.427.682-.673 1.488-.673 2.352s.246 1.67.673 2.352z',
			fill: '#ff8000'
		},
		{
			d: 'M15.101 12c0-.864.247-1.67.674-2.352-.786-1.256-2.183-2.092-3.775-2.092s-2.989.836-3.775 2.092c.427.682.674 1.488.674 2.352s-.247 1.67-.674 2.352c.786 1.256 2.183 2.092 3.775 2.092s2.989-.836 3.775-2.092A4.42 4.42 0 0 1 15.1 12z',
			fill: '#00e054'
		},
		{
			d: 'M19.551 7.556a4.447 4.447 0 0 0-3.775 2.092c.427.682.673 1.488.673 2.352s-.246 1.67-.673 2.352a4.447 4.447 0 0 0 3.775 2.092C22.008 16.444 24 14.454 24 12s-1.992-4.444-4.45-4.444z',
			fill: '#40bcf4'
		}
	];

	/**
	 * The sites above, in order. When none of those MDBList carries is in (its
	 * daily limit, or nothing found yet), TMDB's own score stands in, so a page
	 * is never bare.
	 */
	const shown = $derived.by(() => {
		const by = (source: string) => scores.find((s) => s.source === source && s.score > 0);
		const list = MAIN.map(by).filter((s): s is TitleScore => !!s);
		if (!LIMITED.some((source) => by(source))) {
			const tmdb = by('tmdb');
			if (tmdb) list.push(tmdb);
		}
		return list;
	});

	const percent = (s: TitleScore) => Math.round(Math.max(0, Math.min(100, s.score)));

	function votes(s: TitleScore): string {
		if (!s.votes) return '';
		return ` from ${s.votes.toLocaleString()} ${s.votes === 1 ? 'rating' : 'ratings'}`;
	}

	/** Metacritic colours its score by how good it is. */
	const metaShade = (n: number) => (n >= 61 ? '#00ce7a' : n >= 40 ? '#ffbd3f' : '#ff6874');
</script>

{#if shown.length}
	<ul class="scores" aria-label="Scores on other sites">
		{#each shown as s, i (s.source)}
			{@const n = percent(s)}
			<li title="{NAMES[s.source]}: {n}%{votes(s)}" style="--i: {i}">
				<span class="mark" aria-hidden="true">
					{#if s.source === 'imdb'}
						<svg viewBox="0 0 24 24" width="22" height="22">
							<rect x="0.3" y="0.3" width="23.4" height="23.4" rx="1.8" fill="#f5c518" />
							<path d={IMDB} fill="#000" />
						</svg>
					{:else if s.source === 'tomatoes'}
						<!-- Fresh is red; rotten goes green, as on the site. -->
						<svg viewBox="0 0 24 24" width="22" height="22">
							<path d={TOMATO} fill={n >= 60 ? '#fa320a' : '#0ac855'} />
						</svg>
					{:else if s.source === 'metacritic'}
						<svg viewBox="0 0 24 24" width="22" height="22">
							<circle cx="12" cy="12" r="11.6" fill={metaShade(n)} />
							<path d={METACRITIC} fill="#000" />
						</svg>
					{:else if s.source === 'anilist'}
						<svg viewBox="-3 -3 30 30" width="22" height="22">
							<rect x="-3" y="-3" width="30" height="30" rx="6" fill="#2b2d42" />
							<path d={ANILIST} fill="#02a9ff" />
						</svg>
					{:else if s.source === 'myanimelist'}
						<svg viewBox="-2 -2 28 28" width="22" height="22">
							<rect x="-2" y="-2" width="28" height="28" rx="5" fill="#2e51a2" />
							<path d={MAL} fill="#fff" />
						</svg>
					{:else if s.source === 'letterboxd'}
						<svg viewBox="-2 -2 28 28" width="22" height="22">
							<rect x="-2" y="-2" width="28" height="28" rx="5" fill="#202830" />
							{#each LETTERBOXD as dot (dot.fill)}
								<path d={dot.d} fill={dot.fill} />
							{/each}
						</svg>
					{:else if s.source === 'tmdb'}
						<svg viewBox="-2 -2 28 28" width="22" height="22">
							<defs>
								<linearGradient id="tmdb-mark" x1="0" x2="1">
									<stop offset="0" stop-color="#90cea1" />
									<stop offset="1" stop-color="#01b4e4" />
								</linearGradient>
							</defs>
							<rect x="-2" y="-2" width="28" height="28" rx="5" fill="#0d253f" />
							<path d={TMDB} fill="url(#tmdb-mark)" />
						</svg>
					{/if}
				</span>
				<strong>{n}<small>%</small></strong>
				<span class="site">{NAMES[s.source]}</span>
			</li>
		{/each}
	</ul>
{/if}

<style>
	/* Plain marks and numbers, set like the line of facts above them. */
	.scores {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 20px;
		margin: 2px 0 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		animation: score-in 420ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		animation-delay: calc(var(--i) * 60ms);
	}

	@keyframes score-in {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		li {
			animation: none;
		}
	}

	.mark {
		display: flex;
	}

	.mark svg {
		display: block;
		border-radius: 5px;
	}

	strong {
		font-family: var(--font-display);
		font-size: 0.98rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
		color: var(--text);
		line-height: 1;
	}

	small {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--text-dim);
		margin-left: 1px;
	}

	/* The mark says which site; the name is there for anyone who does not
	   know the mark, and for screen readers. */
	.site {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
</style>
