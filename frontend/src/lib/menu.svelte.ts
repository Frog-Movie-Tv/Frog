import type { IconName } from './components/Icon.svelte';

/*
 * The menu a right-click opens, or a hold on a touch screen.
 *
 * One menu, mounted once in the layout (`ContextMenu.svelte`) and driven from
 * here: a card says what it offers with `use:contextMenu`, and the menu draws
 * it at the pointer on a computer or as a sheet along the bottom of a phone.
 * What a title or a shelf offers is built in `lib/menus.ts`, so every page
 * that shows one offers the same things, in the same words.
 */

export interface MenuItem {
	label: string;
	icon?: IconName;
	/** Drawn in the warning colour: removing, deleting. */
	danger?: boolean;
	/** Ticked, for a choice that is on now. */
	checked?: boolean;
	disabled?: boolean;
	/** Opens a second level in the same menu, read when it is opened. */
	children?: () => MenuEntry[] | Promise<MenuEntry[]>;
	run?: () => void | Promise<void>;
}

/** A line between groups. */
export type MenuEntry = MenuItem | 'rule';

export interface MenuSource {
	/** Named at the top of the menu, so a sheet says what it is about. */
	title?: string;
	/** Built when the menu opens, so it reads the card as it is now. */
	items: () => MenuEntry[];
}

export interface OpenMenu {
	source: MenuSource;
	/** Where the pointer was, for a menu that opens there. */
	x: number;
	y: number;
	/** Opened by a hold on a touch screen: a sheet, whatever the width. */
	touch: boolean;
}

class Menu {
	open = $state<OpenMenu | null>(null);

	show(source: MenuSource, at: { x: number; y: number }, touch = false) {
		this.open = { source, x: at.x, y: at.y, touch };
	}

	close() {
		this.open = null;
	}
}

export const menu = new Menu();

/** How long a finger holds before the menu opens, where the browser does not say. */
const HOLD_MS = 450;
/** A finger that travels this far is scrolling, not holding. */
const SLOP = 10;

/**
 * `use:contextMenu={source}`: a right-click, or a hold on a touch screen,
 * opens the menu the source describes. Undefined does nothing, so a card can
 * take a menu as an optional prop.
 */
export function contextMenu(node: HTMLElement, source: MenuSource | undefined) {
	let current = source;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let startX = 0;
	let startY = 0;
	/** The hold opened the menu: the tap that ends it must not open the card. */
	let held = false;
	/** What last pressed the card, since the menu event itself does not say. */
	let touching = false;

	// A held link on iOS offers a preview of its own; this menu stands in.
	node.style.setProperty('-webkit-touch-callout', 'none');

	const stop = () => {
		if (timer) clearTimeout(timer);
		timer = null;
	};

	const onContextMenu = (e: MouseEvent) => {
		if (!current) return;
		e.preventDefault();
		// Android fires this for a hold as well; the timer may have got there first.
		if (held) return;
		stop();
		// The keyboard's menu key has no place on the page: open on the card.
		const r = node.getBoundingClientRect();
		const at =
			e.clientX || e.clientY ? { x: e.clientX, y: e.clientY } : { x: r.left + 16, y: r.top + 16 };
		menu.show(current, at, touching);
	};

	const onPointerDown = (e: PointerEvent) => {
		touching = e.pointerType === 'touch';
		if (!current || !touching || !e.isPrimary) return;
		held = false;
		startX = e.clientX;
		startY = e.clientY;
		stop();
		timer = setTimeout(() => {
			timer = null;
			if (!current) return;
			held = true;
			navigator.vibrate?.(8);
			menu.show(current, { x: startX, y: startY }, true);
		}, HOLD_MS);
	};

	const onPointerMove = (e: PointerEvent) => {
		if (!timer) return;
		if (Math.abs(e.clientX - startX) > SLOP || Math.abs(e.clientY - startY) > SLOP) stop();
	};

	const onClick = (e: MouseEvent) => {
		if (!held) return;
		e.preventDefault();
		e.stopPropagation();
		held = false;
	};

	node.addEventListener('contextmenu', onContextMenu);
	node.addEventListener('pointerdown', onPointerDown);
	node.addEventListener('pointermove', onPointerMove);
	node.addEventListener('pointerup', stop);
	node.addEventListener('pointercancel', stop);
	node.addEventListener('click', onClick, true);

	return {
		update(next: MenuSource | undefined) {
			current = next;
		},
		destroy() {
			stop();
			node.removeEventListener('contextmenu', onContextMenu);
			node.removeEventListener('pointerdown', onPointerDown);
			node.removeEventListener('pointermove', onPointerMove);
			node.removeEventListener('pointerup', stop);
			node.removeEventListener('pointercancel', stop);
			node.removeEventListener('click', onClick, true);
		}
	};
}
