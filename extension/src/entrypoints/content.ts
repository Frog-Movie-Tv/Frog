// Runs in every frame of every page, and does nothing until a video plays.
//
// Then, every two seconds while something plays, it tells the background
// script about the biggest playing video in this frame and what the frame
// shows about itself (frame report). Frames without a video are asked for
// their report when another frame of the same tab starts playing. Nothing is
// sent anywhere else; the background decides what reaches Frog.

import { browser } from 'wxt/browser';
import { defineContentScript } from 'wxt/utils/define-content-script';
import { pick, type VideoSample } from '../lib/playback.ts';
import type { FrameReport } from '../lib/signals.ts';
import { hideToast, showToast } from '../lib/toast.ts';
import type { FromContent, ToContent } from '../lib/types.ts';

const TICK_MS = 2000;
/** Ticks with nothing playing before the sampler stops. */
const IDLE_TICKS = 15;

export default defineContentScript({
	matches: ['<all_urls>'],
	allFrames: true,
	matchAboutBlank: true,
	matchOriginAsFallback: true,
	runAt: 'document_idle',
	main() {
		const roots = new Set<Document | ShadowRoot>([document]);
		const ids = new WeakMap<HTMLVideoElement, string>();
		// Fresh with every page load, so watching the same episode again after a
		// reload is a new sitting, not the one already logged.
		const load = Math.random().toString(36).slice(2, 8);
		let nextId = 1;
		let timer: ReturnType<typeof setInterval> | null = null;
		let idle = 0;
		let lastReport = '';
		let lastVideo: string | null = null;

		const send = (msg: FromContent) => browser.runtime.sendMessage(msg).catch(() => {});

		// --- finding videos, shadow roots included ---------------------------------

		function listen(root: Document | ShadowRoot) {
			root.addEventListener('playing', start, true);
			root.addEventListener('ended', () => tick(true), true);
		}
		listen(document);

		function adopt(el: Element) {
			const shadow = el.shadowRoot;
			if (shadow && !roots.has(shadow)) {
				roots.add(shadow);
				listen(shadow);
				walk(shadow);
			}
		}

		function walk(root: ParentNode) {
			for (const el of root.querySelectorAll('*')) adopt(el);
		}

		// Open shadow roots, now and as the page adds them, so a player inside a
		// web component is heard when it starts. Only added nodes are looked at.
		walk(document);
		new MutationObserver((records) => {
			for (const r of records) {
				for (const n of r.addedNodes) {
					if (n.nodeType !== 1) continue;
					adopt(n as Element);
					walk(n as Element);
				}
			}
		}).observe(document, { childList: true, subtree: true });

		function videos(): HTMLVideoElement[] {
			const out: HTMLVideoElement[] = [];
			for (const root of roots) {
				if (root instanceof ShadowRoot && !root.host.isConnected) {
					roots.delete(root);
					continue;
				}
				out.push(...root.querySelectorAll('video'));
			}
			return out;
		}

		function sample(v: HTMLVideoElement): VideoSample {
			let id = ids.get(v);
			if (!id) ids.set(v, (id = `${load}-${nextId++}`));
			const r = v.getBoundingClientRect();
			const w = Math.max(0, Math.min(r.right, innerWidth) - Math.max(r.left, 0));
			const h = Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 0));
			const fs = document.fullscreenElement;
			return {
				id,
				src: v.currentSrc || v.src || '',
				duration: v.duration,
				position: v.currentTime,
				playing: !v.paused && !v.ended && v.readyState > 2,
				ended: v.ended,
				area: Math.round(w * h),
				fullscreen: !!fs && (fs === v || fs.contains(v))
			};
		}

		// --- what this frame shows about itself ---------------------------------

		function report(): FrameReport {
			const jsonld: unknown[] = [];
			for (const s of document.querySelectorAll('script[type="application/ld+json"]')) {
				try {
					jsonld.push(JSON.parse(s.textContent ?? ''));
				} catch {
					// A broken block on the page is the page's business.
				}
			}
			const meta: Record<string, string> = {};
			for (const m of document.querySelectorAll('meta[property], meta[name]')) {
				const key = (m.getAttribute('property') ?? m.getAttribute('name') ?? '').toLowerCase();
				if (/^(og:|video:|twitter:title)/.test(key)) meta[key] = m.getAttribute('content') ?? '';
			}
			const ms = navigator.mediaSession?.metadata;
			return {
				url: location.href,
				iframes: [...document.querySelectorAll('iframe')]
					.map((f) => f.src)
					.filter((s) => /^https?:/.test(s)),
				jsonld,
				meta,
				mediaSession: ms ? { title: ms.title, artist: ms.artist, album: ms.album } : null,
				title: document.title,
				headings: [...document.querySelectorAll('h1, h2')]
					.slice(0, 4)
					.map((h) => (h as HTMLElement).innerText?.trim() ?? '')
					.filter(Boolean)
			};
		}

		/** The report only when it changed, so a playing tab sends little. */
		function freshReport(force = false): FrameReport | null {
			const r = report();
			const key = JSON.stringify(r);
			if (!force && key === lastReport) return null;
			lastReport = key;
			return r;
		}

		// --- sampling while something plays ------------------------------------------

		function tick(force = false) {
			const best = pick(videos().map(sample));
			if (!best) idle++;
			else idle = 0;
			const key = best ? `${best.id}|${best.playing}|${best.ended}` : null;
			const r = freshReport();
			if (best || r || key !== lastVideo || force) send({ type: 'report', report: r, video: best });
			lastVideo = key;
			if (idle >= IDLE_TICKS && timer) {
				clearInterval(timer);
				timer = null;
			}
		}

		function start() {
			idle = 0;
			if (!timer) {
				timer = setInterval(tick, TICK_MS);
				tick(true);
			}
		}

		// A video already playing when the script arrived.
		if (videos().some((v) => !v.paused && !v.ended)) start();

		// --- what the background asks of this frame ------------------------------------

		browser.runtime.onMessage.addListener((raw: unknown) => {
			const msg = raw as ToContent | { type: 'collect' } | { type: 'hide' };
			if (msg.type === 'collect') {
				send({ type: 'report', report: freshReport(true), video: pick(videos().map(sample)) });
			} else if (msg.type === 'toast') {
				showToast(msg.text, msg.actions, msg.seconds, (action) =>
					send({ type: 'toast-action', toast: msg.id, action })
				);
			} else if (msg.type === 'hide') {
				hideToast();
			}
		});
	}
});
