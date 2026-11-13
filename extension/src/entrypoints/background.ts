// The extension's one place that decides. Frames report what plays and what
// they show; this keeps each tab's picture of it, asks Frog what it is, logs
// it at the right moment, and keeps every watch in a queue until Frog has it.
//
// Only what resolve and log need is ever sent to Frog: ids, numbers, names
// and the page's title. Never an address, never anything about other tabs.

import { browser, type Browser } from 'wxt/browser';
import { defineBackground } from 'wxt/utils/define-background';
import { frog, FrogError } from '../lib/frog.ts';
import { sittingKey, watched, type VideoSample } from '../lib/playback.ts';
import { backoff, due, label, outcome } from '../lib/queue.ts';
import { domainOf, learn, preferenceOf } from '../lib/rules.ts';
import {
	cluesFrom,
	hasAnything,
	merge,
	namesAgree,
	nameFromTitle,
	signalsKey,
	type Clue,
	type FrameReport,
	type Merged
} from '../lib/signals.ts';
import * as store from '../lib/storage.ts';
import type {
	Candidate,
	FromContent,
	FromPopup,
	Match,
	Pending,
	Resolution,
	TabView,
	ToastAction,
	ToContent
} from '../lib/types.ts';

/** How long Undo stays on the page after a log. */
const UNDO_SECONDS = 10;
const ASK_SECONDS = 30;

interface Frame {
	report: FrameReport | null;
	video: VideoSample | null;
}

interface Tab {
	frames: Map<number, Frame>;
	domain: string | null;
	pageTitle: string;
	sitting: string | null;
	/** The frame playing, which draws the toast when it is full screen. */
	playing: number | null;
	clues: Clue[];
	merged: Merged | null;
	resolveKey: string | null;
	resolution: Resolution | null;
	state: TabView['state'];
	/** This sitting has been logged, or asked about, already. */
	acted: boolean;
	progress: number | null;
	logId: number | null;
	pendingId: string | null;
}

const tabs = new Map<number, Tab>();
/** Which tab and what a toast on screen is about. */
const toasts = new Map<string, number>();

function tabOf(id: number): Tab {
	let t = tabs.get(id);
	if (!t) {
		t = {
			frames: new Map(),
			domain: null,
			pageTitle: '',
			sitting: null,
			playing: null,
			clues: [],
			merged: null,
			resolveKey: null,
			resolution: null,
			state: 'idle',
			acted: false,
			progress: null,
			logId: null,
			pendingId: null
		};
		tabs.set(id, t);
	}
	return t;
}

const id = () => crypto.randomUUID();

// --- the badge ----------------------------------------------------------------------

const COLOURS = { exact: '#32d74b', likely: '#ffb340', guess: '#9a9aa6', unknown: '#ff6961' };

function short(m: Match): string {
	if (m.kind === 'movie') return 'Film';
	if (m.episode == null) return 'TV';
	const full = `S${m.season}E${m.episode}`;
	return full.length <= 4 ? full : `E${m.episode}`;
}

async function badge(tabId: number) {
	const t = tabs.get(tabId);
	const m = t?.resolution?.match ?? null;
	let text = '';
	let colour = COLOURS.guess;
	let title = 'Frog';
	if (t && t.state !== 'idle') {
		if (t.state === 'resolving') {
			text = '…';
		} else if (m) {
			text = short(m);
			colour = COLOURS[m.confidence];
			const what = label(m.name, m.season, m.episode);
			title =
				t.state === 'logged'
					? `Logged ${what}`
					: t.state === 'queued'
						? `${what}, waiting to reach Frog`
						: `${what} (${m.confidence === 'exact' ? 'sure' : m.confidence === 'likely' ? 'likely' : 'a guess'})`;
		} else {
			text = '?';
			colour = COLOURS.unknown;
			title = 'What are you watching?';
		}
	}
	try {
		await browser.action.setBadgeText({ tabId, text });
		await browser.action.setBadgeBackgroundColor({ tabId, color: colour });
		await browser.action.setTitle({ tabId, title });
	} catch {
		// The tab closed in the meantime.
	}
}

// --- toasts -----------------------------------------------------------------------

async function toast(tabId: number, text: string, actions: ToastAction[], seconds: number) {
	const t = tabs.get(tabId);
	const playing = t?.playing != null ? t.frames.get(t.playing) : undefined;
	// The page draws it, unless the player is full screen in its own frame.
	const frameId = playing?.video?.fullscreen ? t!.playing! : 0;
	const toastId = id();
	toasts.set(toastId, tabId);
	const msg: ToContent = { type: 'toast', id: toastId, text, actions, seconds };
	try {
		await browser.tabs.sendMessage(tabId, msg, { frameId });
	} catch {
		// No content script there (a browser page): the badge still says it.
	}
}

// --- reading a tab ------------------------------------------------------------------

async function ruleFor(domain: string | null) {
	return domain ? (await store.rules())[domain] : undefined;
}

/** Work out what the tab plays from every frame's report, and ask Frog when it changed. */
async function refresh(tabId: number) {
	const t = tabOf(tabId);
	const clues: Clue[] = [];
	for (const [frameId, f] of t.frames)
		if (f.report) clues.push(...cluesFrom(f.report, frameId === 0));
	t.clues = clues;
	const top = t.frames.get(0)?.report;
	if (top) t.pageTitle = top.title;
	const merged = merge(clues, t.pageTitle, preferenceOf(await ruleFor(t.domain)));
	t.merged = merged;
	const key = signalsKey(merged.signals);
	if (key === t.resolveKey) return;
	t.resolveKey = key;

	if (!hasAnything(merged.signals)) {
		t.resolution = null;
		t.state = 'unknown';
		await badge(tabId);
		return;
	}
	t.state = 'resolving';
	await badge(tabId);
	try {
		const answer = await frog.resolve(await store.settings(), merged.signals);
		if (t.resolveKey !== key) return;
		const m = answer.match;
		// An id read from an address shape is only sure once the names agree.
		if (m && m.confidence === 'exact' && merged.weak) {
			const names = clues.map((c) => c.name).filter(Boolean) as string[];
			if (!names.some((n) => namesAgree(n, m.name))) m.confidence = 'likely';
		}
		t.resolution = answer;
		t.state = t.logId ? 'logged' : t.pendingId ? 'queued' : m ? 'matched' : 'unknown';
	} catch (e) {
		t.resolveKey = null;
		t.state = 'unknown';
		console.warn('Frog could not resolve this page', e instanceof Error ? e.message : e);
	}
	await badge(tabId);
}

async function onReport(
	tabId: number,
	frameId: number,
	url: string | undefined,
	msg: Extract<FromContent, { type: 'report' }>
) {
	const s = await store.settings();
	if (!s.url || !s.token) return;
	const t = tabOf(tabId);
	t.domain = domainOf(url ?? '') ?? t.domain;
	if (t.domain && s.ignored.includes(t.domain)) return;

	const frame = t.frames.get(frameId) ?? { report: null, video: null };
	if (msg.report) frame.report = msg.report;
	// A frame only resends what changed. After the worker restarted, or the
	// tab moved on, ask it for the whole of it again.
	else if (!frame.report)
		browser.tabs.sendMessage(tabId, { type: 'collect' }, { frameId }).catch(() => {});
	frame.video = msg.video;
	t.frames.set(frameId, frame);

	const v = msg.video;
	if (v) {
		const sitting = `${frameId}|${sittingKey(v)}`;
		if (sitting !== t.sitting) {
			// Something new plays: a new episode, or the episode after its ad.
			t.sitting = sitting;
			t.acted = false;
			t.logId = null;
			t.pendingId = null;
			t.playing = frameId;
			if (t.state === 'logged' || t.state === 'queued')
				t.state = t.resolution?.match ? 'matched' : 'idle';
			// Frames with no video still know things: the page around a player.
			browser.tabs.sendMessage(tabId, { type: 'collect' }).catch(() => {});
		}
		t.progress = v.duration > 0 ? Math.min(v.position / v.duration, 1) : null;
	}
	if (msg.report || v) await refresh(tabId);

	if (
		v &&
		t.playing === frameId &&
		!t.acted &&
		t.state !== 'resolving' &&
		watched(v.position, v.duration, v.ended, s.threshold)
	) {
		t.acted = true;
		await decide(tabId);
	}
}

/** The moment it counts as watched: log it, or ask. */
async function decide(tabId: number) {
	const t = tabOf(tabId);
	const s = await store.settings();
	const m = t.resolution?.match;
	if (!m) {
		await toast(
			tabId,
			'What are you watching? Open Frog in the toolbar to pick it.',
			[{ label: 'OK', action: 'dismiss' }],
			ASK_SECONDS
		);
		return;
	}
	const loggable = m.kind === 'movie' || m.episode != null;
	if (m.confidence === 'exact' && loggable && !s.askFirst) {
		await logMatch(tabId, m, m.season, m.episode, false);
		return;
	}
	const what = label(m.name, m.season, m.episode);
	await toast(
		tabId,
		loggable ? `Log ${what}?` : `Watching ${m.name}? Pick the episode in Frog’s toolbar button.`,
		loggable
			? [
					{ label: 'Log', action: 'log', primary: true },
					{ label: 'Not this', action: 'not-this' }
				]
			: [{ label: 'OK', action: 'dismiss' }],
		ASK_SECONDS
	);
}

// --- logging ------------------------------------------------------------------------

/**
 * Log a title on this tab's behalf. `teach` stores what the page showed as
 * this site's rule, which is what confirming or correcting a match means.
 */
async function logMatch(
	tabId: number,
	c: Candidate,
	season: number | null,
	episode: number | null,
	teach: boolean
) {
	const t = tabOf(tabId);
	const what = label(c.name, season, episode);
	const pending: Pending = {
		id: id(),
		title_id: c.title_id ?? undefined,
		tmdb_id: c.title_id ? undefined : c.tmdb?.id,
		kind: c.title_id ? undefined : c.tmdb?.kind,
		season: c.kind === 'tv' && season != null ? season : undefined,
		episode: c.kind === 'tv' && episode != null ? episode : undefined,
		watched_at: new Date().toISOString(),
		label: what,
		tries: 0,
		next: Date.now(),
		tab: tabId
	};
	await store.saveQueue([...(await store.queue()), pending]);
	t.acted = true;
	t.pendingId = pending.id;
	t.state = 'queued';

	if (teach && t.domain && t.merged) {
		const rules = await store.rules();
		rules[t.domain] = learn(rules[t.domain], t.clues, t.merged, {
			title_id: c.title_id ?? 0,
			tmdb: c.tmdb,
			season,
			episode
		});
		await store.saveRules(rules);
		t.resolveKey = null;
	}

	const result = await send(pending);
	if (result.ok) {
		t.logId = result.logId;
		t.pendingId = null;
		t.state = 'logged';
		await toast(tabId, `Logged ${what}`, [{ label: 'Undo', action: 'undo' }], UNDO_SECONDS);
	} else if (result.outcome === 'refused') {
		await toast(
			tabId,
			`Frog could not log ${what}. ${result.message}`,
			[{ label: 'OK', action: 'dismiss' }],
			ASK_SECONDS
		);
	} else {
		await toast(
			tabId,
			result.outcome === 'reconnect'
				? `${what} is saved. Frog did not accept the token; reconnect in the extension’s settings.`
				: `${what} is saved and reaches Frog as soon as it can.`,
			[{ label: 'Undo', action: 'undo' }],
			UNDO_SECONDS
		);
	}
	await badge(tabId);
}

type Sent = { ok: true; logId: number } | { ok: false; outcome: string; message: string };

/** Try one queued watch now, and keep the queue honest about it. */
async function send(p: Pending): Promise<Sent> {
	const s = await store.settings();
	let result: Sent;
	try {
		const answer = await frog.log(s, {
			title_id: p.title_id,
			tmdb_id: p.tmdb_id,
			kind: p.kind,
			season: p.season,
			episode: p.episode,
			watched_at: p.watched_at
		});
		result = { ok: true, logId: answer.log_id };
	} catch (e) {
		const status = e instanceof FrogError ? e.status : 0;
		result = { ok: false, outcome: outcome(status), message: e instanceof Error ? e.message : '' };
	}
	const queue = await store.queue();
	const i = queue.findIndex((q) => q.id === p.id);
	const kept = queue[i];
	if (!kept) return result;
	if (result.ok) {
		queue.splice(i, 1);
	} else if (result.outcome === 'refused') {
		queue[i] = { ...kept, failed: result.message };
	} else {
		const tries = kept.tries + 1;
		queue[i] = { ...kept, tries, next: Date.now() + backoff(tries) };
	}
	await store.saveQueue(queue);
	if (result.ok && p.tab != null) {
		const t = tabs.get(p.tab);
		if (t?.pendingId === p.id) {
			t.pendingId = null;
			t.logId = result.logId;
			t.state = 'logged';
			await badge(p.tab);
		}
	}
	return result;
}

let flushing = false;

/** Everything due in the queue, oldest first. Stops at the first sign Frog is away. */
async function flush(force = false) {
	if (flushing) return;
	flushing = true;
	try {
		const queue = await store.queue();
		const now = force ? Number.MAX_SAFE_INTEGER : Date.now();
		for (const p of due(queue, now)) {
			const r = await send(p);
			if (!r.ok && r.outcome !== 'refused') break;
		}
	} finally {
		flushing = false;
	}
}

async function undo(tabId: number) {
	const t = tabOf(tabId);
	if (t.pendingId) {
		const pending = t.pendingId;
		await store.saveQueue((await store.queue()).filter((q) => q.id !== pending));
		t.pendingId = null;
	} else if (t.logId) {
		await frog.undo(await store.settings(), t.logId);
		t.logId = null;
	}
	t.state = t.resolution?.match ? 'matched' : 'idle';
	await badge(tabId);
}

// --- the popup's view -----------------------------------------------------------------

async function view(tabId: number): Promise<TabView> {
	const t = tabs.get(tabId);
	const s = await store.settings();
	let domain = t?.domain ?? null;
	if (!domain) {
		try {
			domain = domainOf((await browser.tabs.get(tabId)).url ?? '');
		} catch {
			domain = null;
		}
	}
	return {
		domain,
		ignored: !!domain && s.ignored.includes(domain),
		state: t?.state ?? 'idle',
		match: t?.resolution?.match ?? null,
		alternatives: t?.resolution?.alternatives ?? [],
		guess: t?.merged?.signals.title ?? nameFromTitle(t?.pageTitle ?? '') ?? '',
		progress: t?.progress ?? null,
		logId: t?.logId ?? null,
		pendingId: t?.pendingId ?? null,
		hasRule: !!(await ruleFor(domain))
	};
}

async function onPopup(msg: FromPopup): Promise<unknown> {
	switch (msg.type) {
		case 'view':
			return view(msg.tab);
		case 'confirm': {
			const m = tabs.get(msg.tab)?.resolution?.match;
			if (!m) throw new Error('Nothing to log on this tab.');
			await logMatch(msg.tab, m, m.season, m.episode, true);
			return view(msg.tab);
		}
		case 'choose':
			await logMatch(msg.tab, msg.candidate, msg.season, msg.episode, true);
			return view(msg.tab);
		case 'undo':
			await undo(msg.tab);
			return view(msg.tab);
		case 'search': {
			const t = tabs.get(msg.tab);
			const signals = { ...(t?.merged?.signals ?? {}), title: msg.query };
			delete signals.title_id;
			delete signals.tmdb_id;
			delete signals.imdb_id;
			delete signals.page_title;
			const answer = await frog.resolve(await store.settings(), signals);
			return answer;
		}
		case 'site': {
			const s = await store.settings();
			const domain = (await view(msg.tab)).domain;
			if (!domain) return view(msg.tab);
			const ignored = msg.on
				? s.ignored.filter((d) => d !== domain)
				: [...new Set([...s.ignored, domain])];
			await store.saveSettings({ ignored });
			if (!msg.on) {
				tabs.delete(msg.tab);
				await badge(msg.tab);
			}
			return view(msg.tab);
		}
		case 'flush':
			await flush(true);
			return store.queue();
		case 'retry': {
			const queue = (await store.queue()).map((q) =>
				q.id === msg.id ? { ...q, failed: undefined, tries: 0, next: Date.now() } : q
			);
			await store.saveQueue(queue);
			await flush();
			return store.queue();
		}
		case 'discard':
			await store.saveQueue((await store.queue()).filter((q) => q.id !== msg.id));
			return store.queue();
		case 'me':
			return frog.me(await store.settings());
	}
}

// --- wiring -------------------------------------------------------------------------

export default defineBackground(() => {
	browser.runtime.onMessage.addListener((raw: unknown, sender: Browser.runtime.MessageSender) => {
		const msg = raw as FromContent | FromPopup;
		const tabId = sender.tab?.id;
		if (msg.type === 'report' && tabId != null) {
			void onReport(tabId, sender.frameId ?? 0, sender.tab?.url, msg);
			return;
		}
		if (msg.type === 'toast-action') {
			const tab = toasts.get(msg.toast);
			toasts.delete(msg.toast);
			if (tab == null) return;
			if (msg.action === 'undo') void undo(tab).then(() => toast(tab, 'Undone', [], 3));
			if (msg.action === 'log') {
				const m = tabs.get(tab)?.resolution?.match;
				if (m) void logMatch(tab, m, m.season, m.episode, true);
			}
			if (msg.action === 'not-this') {
				const open = (browser.action as { openPopup?: () => Promise<void> }).openPopup;
				void (open ? open() : Promise.reject()).catch(() =>
					toast(tab, 'Open Frog in the toolbar to pick what you are watching.', [], 8)
				);
			}
			return;
		}
		// The popup and the options page: answer with a promise, errors as text.
		return onPopup(msg as FromPopup).then(
			(value) => ({ ok: true, value }),
			(e) => ({ ok: false, error: e instanceof Error ? e.message : String(e) })
		);
	});

	browser.tabs.onRemoved.addListener((tabId) => tabs.delete(tabId));
	browser.tabs.onUpdated.addListener((tabId, change) => {
		// A new page in the tab, a reload included: what the old one showed no longer counts.
		if (change.url || change.status === 'loading') {
			const t = tabs.get(tabId);
			if (t) {
				t.frames.clear();
				t.resolveKey = null;
				t.resolution = null;
				t.state = 'idle';
				t.sitting = null;
				t.acted = false;
				void badge(tabId);
				browser.tabs.sendMessage(tabId, { type: 'collect' }).catch(() => {});
			}
		}
	});

	browser.alarms.create('frog-flush', { periodInMinutes: 1 });
	browser.alarms.onAlarm.addListener((a) => {
		if (a.name === 'frog-flush') void flush();
	});
	browser.runtime.onStartup.addListener(() => void flush());
	void flush();
});
