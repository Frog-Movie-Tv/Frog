/**
 * Doomsday's premiere, about eight seconds.
 *
 * Dark and the letterbox; three heartbeats with the picture striking through
 * like lightning; zero, a held breath, glass cracking across the screen; the
 * pieces drawn in, then blown out past the camera with a split shockwave; the
 * picture punching in with green embers in a vortex and our own sigils
 * turning; the logo dropped from high with a trail and an impact; the line
 * counted up, and the Ready badge landing beside it for whoever made it.
 */

import type { Cast, Stage, Story } from './stage';
import { countUp } from './stage';

export function play(stage: Stage, cast: Cast, story: Story): { handoff: number } {
	const fill = 'forwards' as const;
	const centre = (el: HTMLElement) => {
		const r = el.getBoundingClientRect();
		const base = stage.canvas.getBoundingClientRect();
		return { x: r.left - base.left + r.width / 2, y: r.top - base.top + r.height / 2, r };
	};

	// 1. Dark: the page goes, the letterbox closes in.
	stage.at(0, () => {
		cast.overlay.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 600, fill });
		cast.bars[0].animate([{ transform: 'translateY(-100%)' }, { transform: 'translateY(0)' }], {
			duration: 700,
			delay: 150,
			easing: 'cubic-bezier(.2,.8,.2,1)',
			fill
		});
		cast.bars[1].animate([{ transform: 'translateY(100%)' }, { transform: 'translateY(0)' }], {
			duration: 700,
			delay: 150,
			easing: 'cubic-bezier(.2,.8,.2,1)',
			fill
		});
	});

	// 2. The last three seconds, each a heartbeat, the picture striking
	//    through the dark a little longer each time.
	['03', '02', '01'].forEach((n, i) =>
		stage.at(0.6 + i, () => {
			cast.seconds.textContent = n;
			stage.pushIn(0.04);
			stage.shake(2 + i * 3);
			const c = centre(cast.clock);
			stage.ring({ x: c.x, y: c.y, speed: 700 + i * 150, width: 2, life: 0.7 });
			for (const t of cast.tiles)
				t.animate([{ transform: 'scale(1.1)' }, { transform: 'scale(1)' }], {
					duration: 260,
					easing: 'cubic-bezier(.2,.8,.2,1)'
				});
			const strike = 70 + i * 45;
			cast.backdrop.animate(
				[
					{ opacity: 0.25 + i * 0.15, transform: 'scale(1.3)' },
					{ opacity: 0.25 + i * 0.15, transform: 'scale(1.3)', offset: 0.6 },
					{ opacity: 0, transform: 'scale(1.3)' }
				],
				{ duration: strike }
			);
		})
	);

	// 3. Zero. Everything holds; the digits burn; the glass cracks.
	stage.at(3.6, () => {
		cast.seconds.textContent = '00';
		cast.clock.classList.add('hot');
		const c = centre(cast.clock);
		stage.crack(c.x, c.y, 16, 2200);
	});

	// 4. The pieces drawn in.
	stage.at(3.85, () => {
		stage.shatter(cast.tiles);
		cast.clock.style.visibility = 'hidden';
		stage.implode();
		stage.pushIn(0.02);
	});

	// 5. Detonation.
	stage.at(4.2, () => {
		stage.detonate(1.2);
		stage.flashOut(0.95);
		stage.ring({ split: true, speed: 1700, width: 6 });
		stage.streak(70);
		stage.burst(stage.cx, stage.cy, 140, '#d9ffe8', 900);
		stage.shake(28);
		stage.pushIn(0.06);
	});
	stage.at(4.32, () => stage.ring({ split: true, speed: 1200, width: 3 }));
	stage.at(4.3, () => stage.clearCracks());

	// 6. The picture punches in; the light leaks across; embers and sigils.
	stage.at(4.4, () => {
		cast.backdrop.animate(
			[
				{ opacity: 0, transform: 'scale(1.35)' },
				{ opacity: 1, transform: 'scale(1.06)', offset: 0.35 },
				{ opacity: 1, transform: 'scale(1)' }
			],
			{ duration: 1700, easing: 'cubic-bezier(.12,.9,.2,1)', fill }
		);
		cast.leak.animate(
			[
				{ transform: 'translateX(-60%)', opacity: 0 },
				{ opacity: 0.55, offset: 0.4 },
				{ transform: 'translateX(60%)', opacity: 0 }
			],
			{ duration: 1500, easing: 'ease-in-out' }
		);
		stage.swirl();
		stage.sigils();
	});

	// 7. The logo, dropped from high, with its trail; the impact; the sweep.
	stage.at(5.2, () => {
		const fall = [
			{ transform: 'translateY(-42vh) scale(3.4)', opacity: 0 },
			{ opacity: 1, offset: 0.35 },
			{ transform: 'translateY(0) scale(1)', opacity: 1 }
		];
		const timing = { duration: 420, easing: 'cubic-bezier(.6,0,.9,.45)', fill } as const;
		cast.logo.animate(fall, timing);
		cast.ghosts.forEach((g, i) =>
			g.animate(
				[
					{ ...fall[0] },
					{ opacity: 0.4 - i * 0.12, offset: 0.35 },
					{ transform: 'translateY(0) scale(1)', opacity: 0 }
				],
				{ ...timing, delay: 35 * (i + 1) }
			)
		);
	});
	stage.at(5.62, () => {
		stage.shake(20);
		stage.pushIn(0.035);
		const c = centre(cast.logo);
		const foot = c.y + c.r.height / 2;
		stage.ring({ x: c.x, y: foot, squash: 0.16, speed: 1100, width: 3, color: '#d9ffe8' });
		stage.burst(c.x, foot, 70, '#d9ffe8', 420);
	});
	stage.at(5.9, () => cast.logo.classList.add('sweep'));

	// 8. You: the label opens out, the line counts up, the badge lands.
	stage.at(6.2, () => {
		cast.label.animate(
			[
				{ opacity: 0, letterSpacing: '0em' },
				{ opacity: 1, letterSpacing: '0.14em' }
			],
			{ duration: 1100, easing: 'cubic-bezier(.2,.8,.2,1)', fill }
		);
	});
	stage.at(6.5, () => {
		cast.line.animate(
			[
				{ opacity: 0, transform: 'translateY(10px)' },
				{ opacity: 1, transform: 'none' }
			],
			{ duration: 500, fill }
		);
		countUp(stage, cast.count, 0, Number(cast.count.dataset.to ?? 0), 0.8);
	});
	if (story.ready && cast.badge) {
		const badge = cast.badge;
		stage.at(6.8, () =>
			badge.animate(
				[
					{ transform: 'translate(46vw, -52vh) scale(.3) rotateY(0deg)', opacity: 0 },
					{ opacity: 1, offset: 0.2 },
					{ transform: 'translate(0, 0) scale(1) rotateY(360deg)', opacity: 1 }
				],
				{ duration: 750, easing: 'cubic-bezier(.2,.8,.25,1)', fill }
			)
		);
		stage.at(7.5, () => {
			const c = centre(badge);
			stage.burst(c.x, c.y, 60, story.accent, 380);
			stage.ring({ x: c.x, y: c.y, speed: 500, width: 2, life: 0.6 });
		});
	}
	stage.at(7.1, () =>
		cast.actions.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 400, fill })
	);

	// 9. The handoff: the letterbox opens and the page is simply there.
	stage.at(7.5, () => {
		cast.bars[0].animate([{ transform: 'translateY(0)' }, { transform: 'translateY(-100%)' }], {
			duration: 600,
			fill
		});
		cast.bars[1].animate([{ transform: 'translateY(0)' }, { transform: 'translateY(100%)' }], {
			duration: 600,
			fill
		});
	});
	return { handoff: 7.5 };
}
