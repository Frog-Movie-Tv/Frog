/**
 * The premiere's stage: one canvas over the page, one clock, and the pieces
 * every event's premiere is built from — shards, rings, sparks, embers,
 * light streaks, cracks, sigils, the camera's push and shake, and the glide
 * that hands the picture over to the hero underneath.
 *
 * An event's own choreography (`./{slug}.ts`) only says what happens when.
 * The clock stops while the tab is hidden, the canvas is drawn at most at
 * twice the screen's pixels, and particles are kept to a budget so a phone
 * holds its frame rate. Nothing here loads anything: the pictures are the
 * page's own, through `/img`.
 */

/** Everything the premiere moves, as the component draws it. */
export interface Cast {
	overlay: HTMLElement;
	/** Holds what the camera moves: the backdrop and the canvas. */
	scene: HTMLElement;
	backdrop: HTMLElement;
	/** A wash of light that can cross the picture. */
	leak: HTMLElement;
	clock: HTMLElement;
	tiles: HTMLElement[];
	seconds: HTMLElement;
	body: HTMLElement;
	label: HTMLElement;
	logo: HTMLElement;
	/** Copies of the logo that trail it as it falls. */
	ghosts: HTMLElement[];
	line: HTMLElement;
	count: HTMLElement;
	actions: HTMLElement;
	badge: HTMLElement | null;
	bars: [HTMLElement, HTMLElement];
}

/** What the premiere says about the person watching it. */
export interface Story {
	accent: string;
	/** Every essential seen in time: the Ready badge lands. */
	ready: boolean;
	essentials: number;
}

type Cue = { at: number; run: () => void; done: boolean };
type Hook = (t: number, dt: number) => void;

interface Shard {
	x: number;
	y: number;
	vx: number;
	vy: number;
	z: number;
	vz: number;
	a: number;
	va: number;
	pts: [number, number][];
	life: number;
	px: number;
	py: number;
	pull: boolean;
}
interface Spark {
	x: number;
	y: number;
	vx: number;
	vy: number;
	life: number;
	decay: number;
	size: number;
	color: string;
}
interface Ember {
	a: number;
	r: number;
	w: number;
	y: number;
	vy: number;
	size: number;
	life: number;
}
interface Ring {
	x: number;
	y: number;
	r: number;
	speed: number;
	width: number;
	life: number;
	split: boolean;
	squash: number;
	color: string;
}
interface Streak {
	a: number;
	len: number;
	speed: number;
	life: number;
}
interface Crack {
	pts: [number, number][];
	grow: number;
	shown: number;
}

/** Particles kept at once: about what a Pixel 7 draws at 60 fps. */
const BUDGET = 600;

export const easeOut = (x: number) => 1 - Math.pow(1 - Math.min(1, Math.max(0, x)), 3);
export const easeInOut = (x: number) => {
	const c = Math.min(1, Math.max(0, x));
	return c < 0.5 ? 4 * c * c * c : 1 - Math.pow(-2 * c + 2, 3) / 2;
};

export class Stage {
	readonly cast: Cast;
	readonly canvas: HTMLCanvasElement;
	private g: CanvasRenderingContext2D;
	private w = 0;
	private h = 0;
	private dpr = 1;
	private cues: Cue[] = [];
	private hooks: Hook[] = [];
	private raf = 0;
	private last = 0;
	private stopped = false;
	/** Seconds on the premiere's own clock. */
	t = 0;
	accent: string;

	private shards: Shard[] = [];
	private sparks: Spark[] = [];
	private embers: Ember[] = [];
	private rings: Ring[] = [];
	private streaks: Streak[] = [];
	private cracks: Crack[] = [];
	private flash = 0;
	private shakeAmp = 0;
	private push = 0;
	private sigil = 0;
	private vortex = false;

	/** Frame times in ms, for checking the frame rate. */
	readonly frames: number[] = [];
	private resolveEnd: (() => void) | null = null;
	readonly ended: Promise<void>;

	constructor(cast: Cast, canvas: HTMLCanvasElement, accent: string) {
		this.cast = cast;
		this.canvas = canvas;
		this.accent = accent;
		const g = canvas.getContext('2d');
		if (!g) throw new Error('no canvas');
		this.g = g;
		this.ended = new Promise((r) => (this.resolveEnd = r));
		this.size();
	}

	size() {
		const r = this.canvas.getBoundingClientRect();
		this.dpr = Math.min(2, window.devicePixelRatio || 1);
		this.w = r.width;
		this.h = r.height;
		this.canvas.width = Math.round(r.width * this.dpr);
		this.canvas.height = Math.round(r.height * this.dpr);
		this.g.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
	}

	get width() {
		return this.w;
	}
	get height() {
		return this.h;
	}
	get cx() {
		return this.w / 2;
	}
	get cy() {
		return this.h / 2;
	}

	/** Do this at that second of the premiere. */
	at(seconds: number, run: () => void) {
		this.cues.push({ at: seconds, run, done: false });
		this.cues.sort((a, b) => a.at - b.at);
	}

	/** Run every frame from now on. */
	every(hook: Hook) {
		this.hooks.push(hook);
	}

	start() {
		this.last = performance.now();
		this.raf = requestAnimationFrame(this.frame);
	}

	/** Jump to `seconds`, running every cue before it at once. */
	jump(seconds: number) {
		this.t = Math.max(this.t, seconds);
		this.shards = [];
		this.cracks = [];
		this.flash = 0;
		this.shakeAmp = 0;
		this.push = 0;
		for (const c of this.cues)
			if (!c.done && c.at <= this.t) {
				c.done = true;
				c.run();
			}
	}

	end() {
		if (this.stopped) return;
		this.stopped = true;
		cancelAnimationFrame(this.raf);
		this.resolveEnd?.();
	}

	private frame = (now: number) => {
		if (this.stopped) return;
		const raw = now - this.last;
		this.last = now;
		// A hidden tab stops the clock; a long gap is not a jump forward.
		const dt = document.hidden ? 0 : Math.min(0.05, raw / 1000);
		if (!document.hidden && this.t > 0) this.frames.push(raw);
		this.t += dt;
		for (const c of this.cues) {
			if (c.done || c.at > this.t) continue;
			c.done = true;
			c.run();
		}
		for (const h of this.hooks) h(this.t, dt);
		this.draw(dt);
		this.camera(dt);
		this.raf = requestAnimationFrame(this.frame);
	};

	// --- the camera ------------------------------------------------------------

	/** A push towards the screen that eases back. */
	pushIn(amount: number) {
		this.push = Math.max(this.push, amount);
	}

	shake(amount: number) {
		this.shakeAmp = Math.max(this.shakeAmp, amount);
	}

	private camera(dt: number) {
		this.push *= Math.pow(0.0005, dt);
		this.shakeAmp *= Math.pow(0.004, dt);
		const s = this.shakeAmp;
		const x = s > 0.3 ? (Math.random() - 0.5) * 2 * s : 0;
		const y = s > 0.3 ? (Math.random() - 0.5) * 2 * s : 0;
		const k = 1 + this.push;
		const transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0) scale(${k.toFixed(4)})`;
		this.cast.scene.style.transform = transform;
		this.cast.clock.style.transform = `translate(-50%, -50%) ${transform}`;
	}

	// --- what is drawn ---------------------------------------------------------

	/** White over everything, fading. */
	flashOut(amount: number) {
		this.flash = Math.max(this.flash, amount);
	}

	/** Glass cracking from a point out to the screen's edges. */
	crack(x: number, y: number, branches = 14, speed = 2600) {
		const reach = Math.hypot(this.w, this.h);
		for (let i = 0; i < branches; i++) {
			let a = (i / branches) * Math.PI * 2 + Math.random() * 0.4;
			let px = x;
			let py = y;
			const pts: [number, number][] = [[px, py]];
			let travelled = 0;
			while (travelled < reach) {
				const step = 30 + Math.random() * 50;
				a += (Math.random() - 0.5) * 0.7;
				px += Math.cos(a) * step;
				py += Math.sin(a) * step;
				travelled += step;
				pts.push([px, py]);
			}
			this.cracks.push({ pts, grow: speed * (0.7 + Math.random() * 0.6), shown: 0 });
		}
	}

	/** The glass is gone: its cracks go with it. */
	clearCracks() {
		this.cracks = [];
	}

	/** The tiles broken into pieces where they stand. */
	shatter(els: HTMLElement[], cols = 3, rows = 4) {
		const base = this.canvas.getBoundingClientRect();
		for (const el of els) {
			const r = el.getBoundingClientRect();
			const cw = r.width / cols;
			const ch = r.height / rows;
			for (let i = 0; i < cols; i++)
				for (let j = 0; j < rows; j++) {
					const x0 = r.left - base.left + i * cw;
					const y0 = r.top - base.top + j * ch;
					const jx = (Math.random() - 0.5) * cw * 0.4;
					const jy = (Math.random() - 0.5) * ch * 0.4;
					for (const tri of [
						[
							[x0, y0],
							[x0 + cw + jx, y0],
							[x0, y0 + ch + jy]
						],
						[
							[x0 + cw + jx, y0],
							[x0 + cw, y0 + ch],
							[x0, y0 + ch + jy]
						]
					] as [number, number][][]) {
						const mx = (tri[0][0] + tri[1][0] + tri[2][0]) / 3;
						const my = (tri[0][1] + tri[1][1] + tri[2][1]) / 3;
						this.shards.push({
							x: mx,
							y: my,
							px: mx,
							py: my,
							vx: 0,
							vy: 0,
							z: 0,
							vz: 0,
							a: 0,
							va: (Math.random() - 0.5) * 3,
							pts: tri.map(([px, py]) => [px - mx, py - my]),
							life: 1,
							pull: false
						});
					}
				}
		}
	}

	/** The pieces drawn in towards the middle. */
	implode() {
		for (const s of this.shards) s.pull = true;
	}

	/** And then out past the camera. */
	detonate(power = 1) {
		const cx = this.cx;
		const cy = this.cy;
		for (const s of this.shards) {
			s.pull = false;
			const dx = s.x - cx;
			const dy = s.y - cy;
			const l = Math.hypot(dx, dy) || 1;
			const v = (520 + Math.random() * 900) * power;
			s.vx = (dx / l) * v + (Math.random() - 0.5) * 300;
			s.vy = (dy / l) * v + (Math.random() - 0.5) * 300 - 120;
			// Some come at the camera, larger and smeared.
			s.vz = Math.random() < 0.3 ? 1.8 + Math.random() * 2.4 : Math.random() * 0.6;
			s.va = (Math.random() - 0.5) * 14;
		}
	}

	ring(o: Partial<Ring> = {}) {
		this.rings.push({
			x: this.cx,
			y: this.cy,
			r: 0,
			speed: 1400,
			width: 4,
			life: 1,
			split: false,
			squash: 1,
			color: this.accent,
			...o
		});
	}

	/** Lines of light from the middle outwards. */
	streak(count = 40) {
		for (let i = 0; i < count; i++)
			this.streaks.push({
				a: Math.random() * Math.PI * 2,
				len: 0,
				speed: 1800 + Math.random() * 2200,
				life: 1
			});
	}

	burst(x: number, y: number, count = 40, color = this.accent, speed = 520) {
		for (let i = 0; i < count && this.sparks.length < BUDGET; i++) {
			const a = Math.random() * Math.PI * 2;
			const v = speed * (0.3 + Math.random());
			this.sparks.push({
				x,
				y,
				vx: Math.cos(a) * v,
				vy: Math.sin(a) * v,
				life: 1,
				decay: 0.9 + Math.random() * 0.9,
				size: 1.5 + Math.random() * 2.5,
				color
			});
		}
	}

	/** Embers swirling up around the middle, in a slow vortex. */
	swirl() {
		this.vortex = true;
	}

	/** Rings and ticks turning faintly behind the logo. */
	sigils() {
		this.sigil = 0.0001;
	}

	private draw(dt: number) {
		const g = this.g;
		g.clearRect(0, 0, this.w, this.h);
		const accent = this.accent;

		if (this.sigil > 0) this.drawSigil(dt);

		if (this.vortex && this.embers.length < 220 && Math.random() < 0.9)
			for (let i = 0; i < 3; i++)
				this.embers.push({
					a: Math.random() * Math.PI * 2,
					r: Math.min(this.w, this.h) * (0.25 + Math.random() * 0.45),
					w: (0.25 + Math.random() * 0.5) * (Math.random() < 0.5 ? 1 : -1),
					y: this.h + 10,
					vy: 30 + Math.random() * 80,
					size: 0.8 + Math.random() * 2,
					life: 1
				});

		// Cracks across the glass.
		if (this.cracks.length) {
			g.save();
			g.strokeStyle = 'rgba(225, 255, 236, 0.85)';
			g.lineWidth = 1.4;
			g.shadowColor = accent;
			g.shadowBlur = 6;
			for (const c of this.cracks) {
				c.shown += c.grow * dt;
				let left = c.shown;
				g.beginPath();
				g.moveTo(c.pts[0][0], c.pts[0][1]);
				for (let i = 1; i < c.pts.length && left > 0; i++) {
					const [x0, y0] = c.pts[i - 1];
					const [x1, y1] = c.pts[i];
					const seg = Math.hypot(x1 - x0, y1 - y0);
					const k = Math.min(1, left / seg);
					g.lineTo(x0 + (x1 - x0) * k, y0 + (y1 - y0) * k);
					left -= seg;
				}
				g.stroke();
			}
			g.restore();
		}

		// Shards, nearer ones larger and smeared along their path.
		for (const s of this.shards) {
			s.px = s.x;
			s.py = s.y;
			if (s.pull) {
				s.x += (this.cx - s.x) * Math.min(1, dt * 5);
				s.y += (this.cy - s.y) * Math.min(1, dt * 5);
				s.a += s.va * dt * 2;
			} else {
				s.x += s.vx * dt;
				s.y += s.vy * dt;
				s.vy += 400 * dt;
				s.z += s.vz * dt;
				s.a += s.va * dt;
				if (s.vx || s.vy) s.life -= dt * 0.7;
			}
			if (s.life <= 0) continue;
			const k = 1 + s.z;
			g.save();
			if (s.z > 1) {
				g.globalAlpha = s.life * 0.35;
				g.strokeStyle = accent;
				g.lineWidth = 6 * k;
				g.beginPath();
				g.moveTo(s.px - s.vx * 0.04, s.py - s.vy * 0.04);
				g.lineTo(s.x, s.y);
				g.stroke();
			}
			g.translate(s.x, s.y);
			g.rotate(s.a);
			g.scale(k, k);
			g.globalAlpha = Math.max(0, s.life);
			g.beginPath();
			g.moveTo(s.pts[0][0], s.pts[0][1]);
			g.lineTo(s.pts[1][0], s.pts[1][1]);
			g.lineTo(s.pts[2][0], s.pts[2][1]);
			g.closePath();
			g.fillStyle = '#132019';
			g.fill();
			g.strokeStyle = accent;
			g.lineWidth = 1 / k;
			g.stroke();
			g.restore();
		}
		this.shards = this.shards.filter(
			(s) => s.life > 0 && s.x > -400 && s.x < this.w + 400 && s.y < this.h + 400
		);

		// Light streaks.
		if (this.streaks.length) {
			g.save();
			g.globalCompositeOperation = 'lighter';
			const reach = Math.hypot(this.w, this.h) / 2;
			for (const s of this.streaks) {
				s.len += s.speed * dt;
				s.life -= dt * 1.6;
				if (s.life <= 0) continue;
				const r0 = Math.max(0, s.len - 260);
				const grad = g.createLinearGradient(
					this.cx + Math.cos(s.a) * r0,
					this.cy + Math.sin(s.a) * r0,
					this.cx + Math.cos(s.a) * s.len,
					this.cy + Math.sin(s.a) * s.len
				);
				grad.addColorStop(0, 'rgba(255,255,255,0)');
				grad.addColorStop(1, `rgba(220,255,235,${(s.life * 0.8).toFixed(3)})`);
				g.strokeStyle = grad;
				g.lineWidth = 2;
				g.beginPath();
				g.moveTo(this.cx + Math.cos(s.a) * r0, this.cy + Math.sin(s.a) * r0);
				g.lineTo(
					this.cx + Math.cos(s.a) * Math.min(s.len, reach * 1.6),
					this.cy + Math.sin(s.a) * Math.min(s.len, reach * 1.6)
				);
				g.stroke();
			}
			g.restore();
			this.streaks = this.streaks.filter((s) => s.life > 0);
		}

		// Shockwaves, split red and cyan.
		for (const r of this.rings) {
			r.r += r.speed * dt;
			r.life -= dt * 0.9;
			if (r.life <= 0) continue;
			g.save();
			g.globalCompositeOperation = 'lighter';
			const rings: [number, string][] = r.split
				? [
						[-5, 'rgba(255, 70, 110,'],
						[5, 'rgba(70, 240, 255,'],
						[0, 'rgba(235, 255, 245,']
					]
				: [[0, this.rgba(r.color)]];
			for (const [off, c] of rings) {
				g.strokeStyle = `${c}${(r.life * 0.9).toFixed(3)})`;
				g.lineWidth = r.width;
				g.beginPath();
				g.ellipse(r.x + off, r.y, Math.max(0, r.r), Math.max(0, r.r * r.squash), 0, 0, Math.PI * 2);
				g.stroke();
			}
			g.restore();
		}
		this.rings = this.rings.filter((r) => r.life > 0);

		// Sparks.
		g.save();
		g.globalCompositeOperation = 'lighter';
		for (const p of this.sparks) {
			p.x += p.vx * dt;
			p.y += p.vy * dt;
			p.vx *= Math.pow(0.15, dt);
			p.vy = p.vy * Math.pow(0.15, dt) + 160 * dt;
			p.life -= p.decay * dt;
			if (p.life <= 0) continue;
			g.globalAlpha = p.life;
			g.fillStyle = p.color;
			g.fillRect(p.x, p.y, p.size, p.size);
		}
		this.sparks = this.sparks.filter((p) => p.life > 0);

		// Embers.
		for (const e of this.embers) {
			e.a += e.w * dt;
			e.y -= e.vy * dt;
			e.r *= Math.pow(0.92, dt);
			e.life -= dt * 0.12;
			const x = this.cx + Math.cos(e.a) * e.r;
			const y = e.y + Math.sin(e.a) * e.r * 0.25;
			g.globalAlpha = Math.max(0, Math.min(1, e.life) * 0.85);
			g.fillStyle = accent;
			g.beginPath();
			g.arc(x, y, e.size, 0, Math.PI * 2);
			g.fill();
		}
		this.embers = this.embers.filter((e) => e.life > 0 && e.y > -40);
		g.restore();

		if (this.flash > 0.01) {
			g.fillStyle = `rgba(240, 255, 246, ${this.flash.toFixed(3)})`;
			g.fillRect(0, 0, this.w, this.h);
			this.flash *= Math.pow(0.02, dt);
		}
	}

	private drawSigil(dt: number) {
		this.sigil = Math.min(1, this.sigil + dt * 0.6);
		const g = this.g;
		const r = Math.min(this.w, this.h) * 0.42;
		const turn = this.t * 0.12;
		g.save();
		g.translate(this.cx, this.cy);
		g.globalAlpha = 0.22 * this.sigil;
		g.strokeStyle = this.accent;
		g.lineWidth = 1.5;
		for (const [k, dir] of [
			[1, 1],
			[0.78, -1],
			[0.56, 1]
		] as const) {
			g.save();
			g.rotate(turn * dir * (1.4 - k));
			g.beginPath();
			g.arc(0, 0, r * k, 0, Math.PI * 2);
			g.stroke();
			const ticks = Math.round(48 * k);
			for (let i = 0; i < ticks; i++) {
				const a = (i / ticks) * Math.PI * 2;
				const long = i % 4 === 0;
				g.beginPath();
				g.moveTo(Math.cos(a) * r * k, Math.sin(a) * r * k);
				g.lineTo(
					Math.cos(a) * r * (k - (long ? 0.05 : 0.025)),
					Math.sin(a) * r * (k - (long ? 0.05 : 0.025))
				);
				g.stroke();
			}
			// Three triangles on the middle ring, our own mark.
			if (k === 0.78)
				for (let i = 0; i < 3; i++) {
					const a = (i / 3) * Math.PI * 2;
					const x = Math.cos(a) * r * k;
					const y = Math.sin(a) * r * k;
					const s = r * 0.05;
					g.beginPath();
					g.moveTo(x + Math.cos(a) * s, y + Math.sin(a) * s);
					g.lineTo(x + Math.cos(a + 2.1) * s, y + Math.sin(a + 2.1) * s);
					g.lineTo(x + Math.cos(a - 2.1) * s, y + Math.sin(a - 2.1) * s);
					g.closePath();
					g.stroke();
				}
			g.restore();
		}
		g.restore();
	}

	private rgba(hex: string): string {
		const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
		if (!m) return 'rgba(142, 240, 180,';
		return `rgba(${parseInt(m[1], 16)}, ${parseInt(m[2], 16)}, ${parseInt(m[3], 16)},`;
	}
}

/** A number counted up in an element, from `from` to `to`, over `seconds`. */
export function countUp(stage: Stage, el: HTMLElement, from: number, to: number, seconds: number) {
	const start = stage.t;
	stage.every((t) => {
		const k = easeOut((t - start) / seconds);
		el.textContent = String(Math.round(from + (to - from) * k));
	});
}

/**
 * Glide the stack onto the hero's own place, so the page is simply there when
 * the premiere leaves: measured from where it is to where `target` is.
 */
export function glide(
	el: HTMLElement,
	target: HTMLElement | null,
	seconds: number
): Animation | null {
	if (!target) return null;
	const a = el.getBoundingClientRect();
	const b = target.getBoundingClientRect();
	if (!a.width || !b.width) return null;
	const k = b.width / a.width;
	return el.animate(
		[
			{ transform: 'translate(0, 0) scale(1)', transformOrigin: 'top left' },
			{
				transform: `translate(${b.left - a.left}px, ${b.top - a.top}px) scale(${k})`,
				transformOrigin: 'top left'
			}
		],
		{ duration: seconds * 1000, easing: 'cubic-bezier(.3,.8,.2,1)', fill: 'forwards' }
	);
}
