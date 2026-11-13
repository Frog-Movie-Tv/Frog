// The small card the extension shows on a page: what was logged, with Undo,
// or what it thinks is playing, with a question. It lives in its own shadow
// root with a constructed style sheet, so the page's styles cannot reach it
// and a strict Content-Security-Policy cannot block it.

import type { ToastAction } from './types.ts';

const FROG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="26" height="26" aria-hidden="true"><ellipse cx="32" cy="38" rx="25" ry="20" fill="#6cc46b"/><circle cx="19" cy="20" r="12" fill="#6cc46b"/><circle cx="45" cy="20" r="12" fill="#6cc46b"/><circle cx="19" cy="20" r="8" fill="#fbfaf1"/><circle cx="45" cy="20" r="8" fill="#fbfaf1"/><circle cx="20.2" cy="21" r="4.2" fill="#11231a"/><circle cx="43.8" cy="21" r="4.2" fill="#11231a"/><polyline points="23,41.5 28,45 32,46 36,45 41,41.5" fill="none" stroke="#2c6b40" stroke-width="3" stroke-linecap="round"/></svg>`;

const CSS = `
:host { all: initial; }
.toast {
	position: fixed; right: 20px; bottom: 20px; z-index: 2147483647;
	display: flex; align-items: center; gap: 12px; max-width: min(420px, calc(100vw - 40px));
	padding: 12px 14px; border-radius: 16px;
	background: rgba(27, 28, 37, 0.97); color: #f5f5f7;
	border: 1px solid rgba(255, 255, 255, 0.11);
	box-shadow: 0 18px 40px -16px rgba(0, 0, 0, 0.7);
	font: 500 14px/1.35 -apple-system, BlinkMacSystemFont, 'Inter Variable', system-ui, sans-serif;
	animation: rise 180ms ease-out;
}
.text { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.actions { display: flex; gap: 6px; flex: none; }
button {
	font-family: inherit; font-size: 13px; font-weight: 600; line-height: 1; cursor: pointer;
	padding: 8px 12px; border-radius: 999px; border: 0;
	background: rgba(255, 255, 255, 0.1); color: #f5f5f7;
}
button.primary { background: #3d9bff; color: #fff; }
@media (prefers-color-scheme: light) {
	.toast { background: rgba(250, 250, 253, 0.98); color: #111216; border-color: rgba(20, 24, 40, 0.1); }
	button:not(.primary) { color: #111216; background: rgba(20, 24, 40, 0.06); }
}
button:focus-visible { outline: 2px solid #3d9bff; outline-offset: 2px; }
@keyframes rise { from { transform: translateY(8px); opacity: 0; } }
@media (prefers-reduced-motion: reduce) { .toast { animation: none; } }
`;

let current: { host: HTMLElement; timer: ReturnType<typeof setTimeout> } | null = null;

export function hideToast() {
	if (!current) return;
	clearTimeout(current.timer);
	current.host.remove();
	current = null;
}

export function showToast(
	text: string,
	actions: ToastAction[],
	seconds: number,
	onAction: (action: string) => void
) {
	hideToast();
	const host = document.createElement('frog-toast');
	host.setAttribute('data-frog', 'toast');
	const root = host.attachShadow({ mode: 'open' });
	const sheet = new CSSStyleSheet();
	sheet.replaceSync(CSS);
	root.adoptedStyleSheets = [sheet];

	const card = document.createElement('div');
	card.className = 'toast';
	card.setAttribute('role', 'status');
	// Parsed as SVG, never as HTML, so a page that enforces Trusted Types has nothing to refuse.
	const mark = new DOMParser().parseFromString(FROG, 'image/svg+xml').documentElement;
	card.append(document.importNode(mark, true));
	const span = document.createElement('span');
	span.className = 'text';
	span.textContent = text;
	card.append(span);
	const row = document.createElement('div');
	row.className = 'actions';
	for (const a of actions) {
		const b = document.createElement('button');
		b.textContent = a.label;
		if (a.primary) b.className = 'primary';
		b.addEventListener('click', (e) => {
			e.stopPropagation();
			hideToast();
			onAction(a.action);
		});
		row.append(b);
	}
	card.append(row);
	root.append(card);

	// In full screen only what is inside the full-screen element is drawn.
	const fs = document.fullscreenElement;
	(fs && fs.tagName !== 'IFRAME' ? fs : document.documentElement).append(host);
	current = { host, timer: setTimeout(hideToast, seconds * 1000) };

	// Tests and assistive tools read the toast through this, since the root is closed.
	host.setAttribute('aria-label', text);
	host.dataset.actions = actions.map((a) => a.label).join('|');
}
