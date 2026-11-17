/**
 * The premiere any event gets without one of its own: the last seconds, the
 * clock breaking, the picture, the logo and the line. About five seconds.
 */

import type { Cast, Stage, Story } from './stage';
import { countUp } from './stage';

export function play(stage: Stage, cast: Cast, story: Story): { handoff: number } {
	const fade = (el: HTMLElement, ms: number, from = 0) =>
		el.animate([{ opacity: from }, { opacity: 1 }], { duration: ms, fill: 'forwards' });

	stage.at(0, () => fade(cast.overlay, 400));
	['03', '02', '01'].forEach((n, i) =>
		stage.at(0.3 + i * 0.7, () => {
			cast.seconds.textContent = n;
			stage.pushIn(0.02);
			for (const t of cast.tiles)
				t.animate([{ transform: 'scale(1.06)' }, { transform: 'scale(1)' }], { duration: 220 });
		})
	);
	stage.at(2.4, () => {
		cast.seconds.textContent = '00';
		stage.shatter(cast.tiles);
		cast.clock.style.visibility = 'hidden';
		stage.detonate(0.8);
		stage.flashOut(0.6);
		stage.ring({ speed: 1300, width: 3 });
		stage.shake(10);
	});
	stage.at(2.6, () =>
		cast.backdrop.animate(
			[
				{ opacity: 0, transform: 'scale(1.15)' },
				{ opacity: 1, transform: 'scale(1)' }
			],
			{ duration: 1400, easing: 'cubic-bezier(.2,.8,.2,1)', fill: 'forwards' }
		)
	);
	stage.at(3.1, () =>
		cast.logo.animate(
			[
				{ opacity: 0, transform: 'scale(1.3)' },
				{ opacity: 1, transform: 'scale(1)' }
			],
			{ duration: 700, easing: 'cubic-bezier(.2,1.3,.4,1)', fill: 'forwards' }
		)
	);
	stage.at(3.8, () => {
		fade(cast.label, 500);
		fade(cast.line, 500);
		countUp(stage, cast.count, 0, Number(cast.count.dataset.to ?? 0), 0.6);
	});
	stage.at(4.2, () => fade(cast.actions, 500));
	void story;
	return { handoff: 4.8 };
}
