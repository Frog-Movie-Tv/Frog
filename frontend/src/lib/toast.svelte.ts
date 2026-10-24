import { ApiError } from './api';

export interface ToastAction {
	label: string;
	run: () => void | Promise<void>;
}

export interface Toast {
	id: number;
	message: string;
	tone: 'info' | 'error';
	action?: ToastAction;
}

let nextId = 1;

class Toasts {
	items = $state<Toast[]>([]);

	push(message: string, tone: Toast['tone'] = 'info') {
		this.add({ message, tone });
	}

	/**
	 * A toast that can take itself back.
	 *
	 * Anything destructive should offer this rather than a confirmation
	 * dialogue: undo costs one click when you were wrong, a dialogue costs one
	 * click every single time you were right.
	 */
	undoable(message: string, undo: () => void | Promise<void>) {
		this.add({ message, tone: 'info', action: { label: 'Undo', run: undo } });
	}

	error(e: unknown) {
		// A 401 is not news: the layout is already sending you to the sign-in
		// screen, and a toast on top of that just looks like a bug.
		if (e instanceof ApiError && e.isAuth) return;
		this.add({ message: e instanceof Error ? e.message : String(e), tone: 'error' });
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
	}

	private add(toast: Omit<Toast, 'id'>) {
		const id = nextId++;
		this.items = [...this.items, { ...toast, id }];
		// An offer to undo needs long enough to notice and reach.
		const life = toast.tone === 'error' ? 6000 : toast.action ? 7000 : 3000;
		setTimeout(() => this.dismiss(id), life);
	}
}

export const toasts = new Toasts();
