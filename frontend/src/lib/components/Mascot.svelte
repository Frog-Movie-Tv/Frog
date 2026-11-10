<script lang="ts" module>
	/** What Pip is doing; each is one class on the drawing. */
	export type Pose = 'idle' | 'lost' | 'asleep' | 'searching' | 'happy' | 'wave' | 'dizzy' | 'hop';
</script>

<script lang="ts">
	import { settings } from '#lib/settings.svelte';

	/**
	 * Pip, the frog: the icon's head on a whole frog, sat on a lily pad.
	 *
	 * One drawing, posed by CSS. Every part is in the markup and a pose shows
	 * its own eyes, mouth and arm and moves them, so changing the pose is one
	 * class. Pip wears whatever mascot is chosen in Settings: the bow, the
	 * sprout, the glasses, the popcorn. The colours are the icon's own, so Pip
	 * is the same frog in every appearance.
	 */

	type Wear = 'classic' | 'bow' | 'sprout' | 'cinema' | 'popcorn';

	interface Props {
		pose?: Pose;
		size?: number;
		/** Said to a screen reader; left out, Pip is decoration. */
		label?: string;
	}

	let { pose = 'idle', size = 160, label }: Props = $props();

	const wear = $derived.by((): Wear => {
		const m = settings.current.mascot;
		return m === 'bow' || m === 'sprout' || m === 'cinema' || m === 'popcorn' ? m : 'classic';
	});
	// With animations off Pip holds still, rather than flicker through a
	// loop the page has shortened to nothing.
	const still = $derived(settings.current.animations === 'off');
</script>

<svg
	class="pip pose-{pose} wear-{wear}"
	class:still
	viewBox="0 0 200 200"
	width={size}
	height={size}
	role={label ? 'img' : undefined}
	aria-label={label || undefined}
	aria-hidden={label ? undefined : true}
>
	<!-- ground: the lily pad, and what the frog casts on it -->
	<g class="pad">
		<path d="M100 172 L179.2 166.3 A82 22 0 1 0 179.2 177.7 Z" fill="#3f9a55" />
		<path d="M100 172 L164 167.5 A66 16 0 1 0 164 176.5 Z" fill="#5db86a" opacity="0.5" />
		<ellipse
			class="ripple"
			cx="100"
			cy="172"
			rx="60"
			ry="16"
			fill="none"
			stroke="#5db86a"
			stroke-width="2"
		/>
		<ellipse
			class="ripple two"
			cx="100"
			cy="172"
			rx="60"
			ry="16"
			fill="none"
			stroke="#5db86a"
			stroke-width="2"
		/>
	</g>
	<ellipse class="shadow" cx="100" cy="166" rx="48" ry="9" fill="#0a1a10" opacity="0.22" />

	<g class="frog">
		<!-- back legs -->
		<g class="legs">
			<ellipse cx="60" cy="150" rx="24" ry="14" fill="#4ea85f" transform="rotate(-18 60 150)" />
			<ellipse cx="140" cy="150" rx="24" ry="14" fill="#4ea85f" transform="rotate(18 140 150)" />
			<ellipse cx="50" cy="162" rx="15" ry="6" fill="#6cc46b" />
			<ellipse cx="150" cy="162" rx="15" ry="6" fill="#6cc46b" />
		</g>

		<g class="body">
			<ellipse cx="100" cy="135" rx="42" ry="34" fill="#6cc46b" />
			<ellipse cx="100" cy="142" rx="28" ry="24" fill="#8ad88a" opacity="0.9" />

			<!-- arms -->
			<g class="arm left">
				<path
					d="M78 138 Q70 150 68 162"
					stroke="#4ea85f"
					stroke-width="10"
					stroke-linecap="round"
					fill="none"
				/>
				<ellipse cx="67" cy="164" rx="10" ry="5.5" fill="#6cc46b" />
				<g class="popcorn">
					<circle cx="60" cy="140" r="6" fill="#ffe9a8" />
					<circle cx="70" cy="137" r="5.5" fill="#ffe9a8" />
					<circle cx="65" cy="133" r="5" fill="#ffe9a8" />
					<rect x="52" y="140" width="26" height="24" rx="4" fill="#f5f1e4" />
					<rect x="59" y="140" width="4.5" height="24" rx="1" fill="#e4572e" />
					<rect x="68" y="140" width="4.5" height="24" rx="1" fill="#e4572e" />
				</g>
			</g>
			<g class="arm right">
				<path
					class="arm-down"
					d="M122 138 Q130 150 132 162"
					stroke="#4ea85f"
					stroke-width="10"
					stroke-linecap="round"
					fill="none"
				/>
				<ellipse class="arm-down" cx="133" cy="164" rx="10" ry="5.5" fill="#6cc46b" />
				<g class="arm-up">
					<path
						d="M122 138 Q138 126 150 112"
						stroke="#4ea85f"
						stroke-width="10"
						stroke-linecap="round"
						fill="none"
					/>
					<ellipse
						cx="153"
						cy="108"
						rx="9"
						ry="6.5"
						fill="#6cc46b"
						transform="rotate(-40 153 108)"
					/>
				</g>
				<g class="glass">
					<path
						d="M122 138 Q134 128 142 118"
						stroke="#4ea85f"
						stroke-width="10"
						stroke-linecap="round"
						fill="none"
					/>
					<ellipse cx="143" cy="116" rx="8" ry="6" fill="#6cc46b" transform="rotate(-45 143 116)" />
					<line
						x1="146"
						y1="112"
						x2="153"
						y2="98"
						stroke="#23342b"
						stroke-width="6"
						stroke-linecap="round"
					/>
					<circle cx="158" cy="85" r="17" fill="#fbfaf1" opacity="0.28" />
					<circle cx="158" cy="85" r="17" fill="none" stroke="#23342b" stroke-width="4.5" />
					<path
						d="M148 77 Q152 71 160 70"
						stroke="#fff"
						stroke-width="2.5"
						stroke-linecap="round"
						fill="none"
						opacity="0.7"
					/>
				</g>
			</g>
		</g>

		<g class="head">
			<ellipse cx="100" cy="92" rx="50" ry="40" fill="#6cc46b" />
			<ellipse cx="100" cy="104" rx="28" ry="20" fill="#8ad88a" opacity="0.45" />
			<circle cx="74" cy="60" r="22" fill="#6cc46b" />
			<circle cx="126" cy="60" r="22" fill="#6cc46b" />
			<ellipse class="blush" cx="60" cy="104" rx="9" ry="6" fill="#f4738f" opacity="0.82" />
			<ellipse class="blush" cx="140" cy="104" rx="9" ry="6" fill="#f4738f" opacity="0.82" />

			<!-- sprout, in the dip between the eyes -->
			<g class="sprout">
				<path
					d="M100 56 Q98 44 100 32"
					stroke="#3f8f52"
					stroke-width="4.5"
					stroke-linecap="round"
					fill="none"
				/>
				<ellipse cx="89" cy="30" rx="11" ry="6.5" fill="#9be27a" />
				<ellipse cx="111" cy="30" rx="11" ry="6.5" fill="#9be27a" />
			</g>
			<!-- bow -->
			<g class="bow">
				<ellipse cx="87" cy="42" rx="13" ry="9.5" fill="#ff8fb1" />
				<ellipse cx="113" cy="42" rx="13" ry="9.5" fill="#ff8fb1" />
				<circle cx="100" cy="45" r="8" fill="#e5577f" />
			</g>

			<!-- eyes, open -->
			<g class="eyes open">
				<circle cx="74" cy="61" r="15" fill="#fbfaf1" />
				<circle cx="126" cy="61" r="15" fill="#fbfaf1" />
				<g class="pupils">
					<circle cx="76" cy="63" r="8" fill="#11231a" />
					<circle cx="124" cy="63" r="8" fill="#11231a" />
					<circle cx="72.5" cy="59" r="3.2" fill="#ffffff" />
					<circle cx="120.5" cy="59" r="3.2" fill="#ffffff" />
				</g>
				<circle class="lid" cx="74" cy="61" r="15.5" fill="#6cc46b" />
				<circle class="lid" cx="126" cy="61" r="15.5" fill="#6cc46b" />
			</g>
			<!-- eyes, closed: asleep -->
			<g class="eyes closed">
				<circle cx="74" cy="61" r="15" fill="#fbfaf1" />
				<circle cx="126" cy="61" r="15" fill="#fbfaf1" />
				<path
					d="M63 60 Q74 72 85 60"
					stroke="#11231a"
					stroke-width="5"
					stroke-linecap="round"
					fill="none"
				/>
				<path
					d="M115 60 Q126 72 137 60"
					stroke="#11231a"
					stroke-width="5"
					stroke-linecap="round"
					fill="none"
				/>
			</g>
			<!-- eyes, shut with joy -->
			<g class="eyes joy">
				<path
					d="M61 66 Q74 50 87 66"
					stroke="#11231a"
					stroke-width="6"
					stroke-linecap="round"
					fill="none"
				/>
				<path
					d="M113 66 Q126 50 139 66"
					stroke="#11231a"
					stroke-width="6"
					stroke-linecap="round"
					fill="none"
				/>
			</g>
			<!-- eyes, dizzy -->
			<g class="eyes dizzy">
				<circle cx="74" cy="61" r="15" fill="#fbfaf1" />
				<circle cx="126" cy="61" r="15" fill="#fbfaf1" />
				<path
					class="swirl"
					d="M74 61 L74.3 61.2 L74.3 61.5 L74.0 61.9 L73.4 62.1 L72.7 61.8 L72.2 61.0 L72.1 59.9 L72.8 58.9 L74.0 58.3 L75.5 58.4 L76.9 59.3 L77.7 61.0 L77.4 63.0 L76.1 64.7 L74.0 65.6 L71.6 65.2 L69.5 63.6 L68.5 61.0 L69.0 58.1 L70.9 55.7 L74.0 54.6 L77.4 55.2 L80.1 57.5 L81.3 61.0 L80.6 64.8 L78.0 67.9 L74.0 69.3 L69.7 68.4 L66.3 65.4 L64.8 61.0 L65.8 56.3 L69.1 52.5 L74.0 50.9 L79.2 52.0 L83.3 55.7 L85.0 61.0"
					stroke="#11231a"
					stroke-width="3"
					fill="none"
					stroke-linecap="round"
				/>
				<path
					class="swirl"
					d="M126 61 L126.3 61.2 L126.3 61.5 L126.0 61.9 L125.4 62.1 L124.7 61.8 L124.2 61.0 L124.1 59.9 L124.8 58.9 L126.0 58.3 L127.5 58.4 L128.9 59.3 L129.7 61.0 L129.4 63.0 L128.1 64.7 L126.0 65.6 L123.6 65.2 L121.5 63.6 L120.5 61.0 L121.0 58.1 L122.9 55.7 L126.0 54.6 L129.4 55.2 L132.1 57.5 L133.3 61.0 L132.6 64.8 L130.0 67.9 L126.0 69.3 L121.7 68.4 L118.3 65.4 L116.8 61.0 L117.8 56.3 L121.1 52.5 L126.0 50.9 L131.2 52.0 L135.3 55.7 L137.0 61.0"
					stroke="#11231a"
					stroke-width="3"
					fill="none"
					stroke-linecap="round"
				/>
			</g>

			<!-- 3D glasses -->
			<g class="cinema">
				<rect x="52" y="48" width="42" height="28" rx="8" fill="#e4572e" opacity="0.78" />
				<rect x="106" y="48" width="42" height="28" rx="8" fill="#35b7c9" opacity="0.78" />
				<rect x="92" y="58" width="16" height="6" rx="3" fill="#23342b" />
				<path d="M52 61 L42 64" stroke="#23342b" stroke-width="5" stroke-linecap="round" />
				<path d="M148 61 L158 64" stroke="#23342b" stroke-width="5" stroke-linecap="round" />
			</g>

			<!-- mouths -->
			<path
				class="mouth smile"
				d="M82 100 Q100 117 118 100"
				stroke="#2c6b40"
				stroke-width="5"
				stroke-linecap="round"
				fill="none"
			/>
			<path
				class="mouth small"
				d="M90 104 Q100 110 110 104"
				stroke="#2c6b40"
				stroke-width="4.5"
				stroke-linecap="round"
				fill="none"
			/>
			<path
				class="mouth wavy"
				d="M84 104 q5 -5 10 0 t10 0 t10 0"
				stroke="#2c6b40"
				stroke-width="4.5"
				stroke-linecap="round"
				fill="none"
			/>
			<g class="mouth grin">
				<path d="M82 98 Q100 126 118 98 Z" fill="#2c6b40" />
				<ellipse cx="100" cy="111" rx="7" ry="4" fill="#f4738f" />
			</g>
			<path class="mouth o" d="M93 104 a7 7 0 1 0 14 0 a7 7 0 1 0 -14 0" fill="#2c6b40" />
		</g>
	</g>

	<!-- what floats around the head -->
	<g class="thought">
		<circle cx="152" cy="30" r="17" fill="#fff" opacity="0.95" />
		<circle cx="137" cy="46" r="5" fill="#fff" opacity="0.9" />
		<circle cx="130" cy="55" r="3" fill="#fff" opacity="0.85" />
		<text
			x="152"
			y="38"
			text-anchor="middle"
			font-size="24"
			font-weight="800"
			fill="#11231a"
			font-family="inherit">?</text
		>
	</g>
	<g class="zees" fill="#11231a" font-weight="800" font-family="inherit">
		<text class="z" x="142" y="52" font-size="14">z</text>
		<text class="z two" x="154" y="38" font-size="18">z</text>
		<text class="z three" x="168" y="22" font-size="22">z</text>
	</g>
	<g class="sparkles" fill="#ffd166">
		<path
			class="spark"
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(40 60)"
		/>
		<path
			class="spark two"
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(160 44)"
		/>
		<path
			class="spark three"
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(146 120)"
		/>
		<path
			class="spark four"
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(50 120)"
		/>
	</g>
	<g class="orbit" fill="#ffd166">
		<path
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(100 24) scale(0.9)"
		/>
		<path
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(68 34) scale(0.7)"
		/>
		<path
			d="M0 -6 L1.7 -1.7 L6 0 L1.7 1.7 L0 6 L-1.7 1.7 L-6 0 L-1.7 -1.7 Z"
			transform="translate(132 34) scale(0.7)"
		/>
	</g>
</svg>

<style>
	.pip {
		display: block;
		overflow: visible;
	}
	.pip * {
		transform-box: view-box;
	}

	/* Only one set of eyes and one mouth at a time. */
	.pip .eyes,
	.pip .mouth,
	.pip .arm-up,
	.pip .glass,
	.pip .thought,
	.pip .zees,
	.pip .sparkles,
	.pip .orbit,
	.pip .ripple,
	.pip .popcorn,
	.pip .sprout,
	.pip .bow,
	.pip .cinema {
		display: none;
	}
	.pip .eyes.open,
	.pip .mouth.smile {
		display: block;
	}

	/* What Pip wears follows the mascot chosen in Settings. */
	.pip.wear-bow .bow,
	.pip.wear-sprout .sprout,
	.pip.wear-cinema .cinema,
	.pip.wear-popcorn .popcorn {
		display: block;
	}

	/* --- always on --------------------------------------------------------- */

	.pip .body,
	.pip .head {
		transform-origin: 100px 165px;
		animation: pip-breathe 3s ease-in-out infinite;
	}
	.pip .head {
		transform-origin: 100px 128px;
		animation:
			pip-breathe 3s ease-in-out infinite,
			pip-head-idle 7s ease-in-out infinite;
	}
	.pip .lid {
		transform-origin: center top;
		transform-box: fill-box;
		transform: scaleY(0);
		animation: pip-blink 4.6s infinite;
	}
	.pip .lid + .lid {
		animation-delay: 0.02s;
	}
	.pip .pupils {
		animation: pip-look 7s ease-in-out infinite;
	}

	@keyframes pip-breathe {
		0%,
		100% {
			transform: scale(1, 1);
		}
		50% {
			transform: scale(1.015, 1.03);
		}
	}
	@keyframes pip-head-idle {
		0%,
		100% {
			rotate: 0deg;
		}
		40% {
			rotate: -2.5deg;
		}
		70% {
			rotate: 2deg;
		}
	}
	@keyframes pip-blink {
		0%,
		91%,
		100% {
			transform: scaleY(0);
		}
		94%,
		96% {
			transform: scaleY(1);
		}
	}
	@keyframes pip-look {
		0%,
		30%,
		100% {
			translate: 0 0;
		}
		38%,
		55% {
			translate: -4px 1px;
		}
		63%,
		85% {
			translate: 4px -1px;
		}
	}

	/* --- lost: the 404 ----------------------------------------------------- */

	.pose-lost .head {
		animation:
			pip-breathe 3s ease-in-out infinite,
			pip-tilt 4.4s ease-in-out infinite;
	}
	.pose-lost .pupils {
		animation: pip-dart 4.4s ease-in-out infinite;
	}
	.pose-lost .mouth.smile {
		display: none;
	}
	.pose-lost .mouth.small {
		display: block;
	}
	.pose-lost .thought {
		display: block;
		transform-origin: 134px 52px;
		animation: pip-bubble 4.4s ease-out infinite;
	}
	.pose-lost .pad,
	.pose-lost .shadow,
	.pose-lost .frog {
		animation: pip-drift 6s ease-in-out infinite;
	}
	@keyframes pip-tilt {
		0%,
		15%,
		100% {
			rotate: 0deg;
		}
		30%,
		45% {
			rotate: -9deg;
		}
		60%,
		78% {
			rotate: 8deg;
		}
	}
	@keyframes pip-dart {
		0%,
		15%,
		100% {
			translate: 0 0;
		}
		30%,
		45% {
			translate: -6px 0;
		}
		60%,
		78% {
			translate: 6px 0;
		}
	}
	@keyframes pip-bubble {
		0%,
		50% {
			opacity: 0;
			scale: 0.4;
		}
		58%,
		94% {
			opacity: 1;
			scale: 1;
		}
		100% {
			opacity: 0;
			scale: 1;
		}
	}
	@keyframes pip-drift {
		0%,
		100% {
			translate: 0 0;
		}
		50% {
			translate: 0 3px;
		}
	}

	/* --- asleep ------------------------------------------------------------ */

	.pose-asleep .eyes.open {
		display: none;
	}
	.pose-asleep .eyes.closed {
		display: block;
	}
	.pose-asleep .mouth.smile {
		display: none;
	}
	.pose-asleep .mouth.small {
		display: block;
	}
	.pose-asleep .body,
	.pose-asleep .head {
		animation-duration: 4.8s;
	}
	.pose-asleep .head {
		animation:
			pip-breathe 4.8s ease-in-out infinite,
			pip-doze 4.8s ease-in-out infinite;
	}
	.pose-asleep .zees {
		display: block;
	}
	.pose-asleep .z {
		animation: pip-zee 3.6s ease-out infinite;
	}
	.pose-asleep .z.two {
		animation-delay: 1.2s;
	}
	.pose-asleep .z.three {
		animation-delay: 2.4s;
	}
	@keyframes pip-doze {
		0%,
		100% {
			rotate: 7deg;
		}
		50% {
			rotate: 10deg;
		}
	}
	@keyframes pip-zee {
		0% {
			opacity: 0;
			translate: 0 10px;
		}
		20% {
			opacity: 1;
		}
		80% {
			opacity: 1;
		}
		100% {
			opacity: 0;
			translate: 6px -22px;
		}
	}

	/* --- searching --------------------------------------------------------- */

	.pose-searching .arm.right .arm-down {
		display: none;
	}
	.pose-searching .glass {
		display: block;
		transform-origin: 122px 138px;
		animation: pip-scan 3.2s ease-in-out infinite;
	}
	.pose-searching .head {
		animation:
			pip-breathe 3s ease-in-out infinite,
			pip-lean 3.2s ease-in-out infinite;
	}
	.pose-searching .pupils {
		animation: pip-peer 3.2s ease-in-out infinite;
	}
	.pose-searching .mouth.smile {
		display: none;
	}
	.pose-searching .mouth.o {
		display: block;
	}
	@keyframes pip-scan {
		0%,
		100% {
			rotate: 0deg;
		}
		35% {
			rotate: -18deg;
		}
		70% {
			rotate: 10deg;
		}
	}
	@keyframes pip-lean {
		0%,
		100% {
			rotate: 4deg;
		}
		35% {
			rotate: 9deg;
		}
		70% {
			rotate: 1deg;
		}
	}
	@keyframes pip-peer {
		0%,
		100% {
			translate: 5px 2px;
		}
		35% {
			translate: 7px 0;
		}
		70% {
			translate: 2px 3px;
		}
	}

	/* --- happy ------------------------------------------------------------- */

	.pose-happy .eyes.open {
		display: none;
	}
	.pose-happy .eyes.joy {
		display: block;
	}
	.pose-happy .mouth.smile {
		display: none;
	}
	.pose-happy .mouth.grin {
		display: block;
	}
	.pose-happy .frog {
		transform-origin: 100px 166px;
		animation: pip-jump 1.8s cubic-bezier(0.3, 0, 0.2, 1) infinite;
	}
	.pose-happy .shadow {
		transform-origin: 100px 166px;
		animation: pip-shadow 1.8s cubic-bezier(0.3, 0, 0.2, 1) infinite;
	}
	.pose-happy .sparkles {
		display: block;
	}
	.pose-happy .spark {
		transform-box: fill-box;
		transform-origin: center;
		animation: pip-spark 1.8s ease-out infinite;
	}
	.pose-happy .spark.two {
		animation-delay: 0.12s;
	}
	.pose-happy .spark.three {
		animation-delay: 0.24s;
	}
	.pose-happy .spark.four {
		animation-delay: 0.06s;
	}
	.pose-happy .ripple {
		display: block;
		transform-origin: 100px 172px;
		animation: pip-ripple 1.8s ease-out infinite;
	}
	.pose-happy .ripple.two {
		animation-delay: 0.15s;
	}
	@keyframes pip-jump {
		0%,
		100% {
			translate: 0 0;
			scale: 1 1;
		}
		12% {
			translate: 0 2px;
			scale: 1.08 0.9;
		}
		24% {
			translate: 0 -30px;
			scale: 0.95 1.07;
		}
		40% {
			translate: 0 -36px;
			scale: 1 1;
		}
		56% {
			translate: 0 -8px;
			scale: 0.97 1.05;
		}
		64% {
			translate: 0 0;
			scale: 1.08 0.92;
		}
		76% {
			scale: 1 1;
		}
	}
	@keyframes pip-shadow {
		0%,
		64%,
		100% {
			scale: 1 1;
			opacity: 0.22;
		}
		40% {
			scale: 0.72 0.8;
			opacity: 0.12;
		}
	}
	@keyframes pip-spark {
		0%,
		20% {
			scale: 0;
			opacity: 0;
		}
		36% {
			scale: 1.2;
			opacity: 1;
		}
		64%,
		100% {
			scale: 0;
			opacity: 0;
		}
	}
	@keyframes pip-ripple {
		0%,
		64% {
			scale: 0.6;
			opacity: 0;
		}
		72% {
			opacity: 0.7;
		}
		100% {
			scale: 1.35;
			opacity: 0;
		}
	}

	/* --- wave -------------------------------------------------------------- */

	.pose-wave .arm.right .arm-down {
		display: none;
	}
	.pose-wave .arm-up {
		display: block;
		transform-origin: 122px 138px;
		animation: pip-wave 1.6s ease-in-out infinite;
	}
	.pose-wave .head {
		animation:
			pip-breathe 3s ease-in-out infinite,
			pip-nod 1.6s ease-in-out infinite;
	}
	@keyframes pip-wave {
		0%,
		100% {
			rotate: 0deg;
		}
		25% {
			rotate: 16deg;
		}
		50% {
			rotate: -6deg;
		}
		75% {
			rotate: 14deg;
		}
	}
	@keyframes pip-nod {
		0%,
		100% {
			rotate: -3deg;
		}
		50% {
			rotate: 3deg;
		}
	}

	/* --- dizzy ------------------------------------------------------------- */

	.pose-dizzy .eyes.open {
		display: none;
	}
	.pose-dizzy .eyes.dizzy {
		display: block;
	}
	.pose-dizzy .swirl {
		transform-box: fill-box;
		transform-origin: center;
		animation: pip-spin 2.4s linear infinite;
	}
	.pose-dizzy .mouth.smile {
		display: none;
	}
	.pose-dizzy .mouth.wavy {
		display: block;
	}
	.pose-dizzy .head {
		animation:
			pip-breathe 3s ease-in-out infinite,
			pip-sway 2.4s ease-in-out infinite;
	}
	.pose-dizzy .orbit {
		display: block;
		transform-origin: 100px 34px;
		animation: pip-orbit 3s linear infinite;
	}
	.pose-dizzy .blush {
		opacity: 0.4;
	}
	@keyframes pip-spin {
		to {
			rotate: 360deg;
		}
	}
	@keyframes pip-sway {
		0%,
		100% {
			rotate: -7deg;
		}
		50% {
			rotate: 7deg;
		}
	}
	@keyframes pip-orbit {
		from {
			rotate: 0deg;
			scale: 1 0.45;
		}
		to {
			rotate: 360deg;
			scale: 1 0.45;
		}
	}

	/* --- hop --------------------------------------------------------------- */

	.pose-hop .frog {
		transform-origin: 100px 166px;
		animation: pip-hop 0.9s cubic-bezier(0.3, 0, 0.2, 1) infinite;
	}
	.pose-hop .shadow {
		transform-origin: 100px 166px;
		animation: pip-hop-shadow 0.9s cubic-bezier(0.3, 0, 0.2, 1) infinite;
	}
	.pose-hop .ripple {
		display: block;
		transform-origin: 100px 172px;
		animation: pip-ripple 0.9s ease-out infinite;
	}
	.pose-hop .ripple.two {
		display: none;
	}
	@keyframes pip-hop {
		0%,
		100% {
			translate: 0 0;
			scale: 1 1;
		}
		18% {
			scale: 1.06 0.92;
		}
		50% {
			translate: 0 -14px;
			scale: 0.97 1.04;
		}
		82% {
			translate: 0 0;
			scale: 1.05 0.94;
		}
	}
	@keyframes pip-hop-shadow {
		0%,
		100% {
			scale: 1 1;
		}
		50% {
			scale: 0.85 0.9;
			opacity: 0.16;
		}
	}

	/* Held still: animations off in Settings, or asked for by the system. */
	.still :global(*) {
		animation: none !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.pip :global(*) {
			animation: none !important;
		}
	}
</style>
