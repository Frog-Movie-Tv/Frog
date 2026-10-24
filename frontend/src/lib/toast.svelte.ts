import { ApiError } from './api';

export interface Toast {
	id: number;
	message: string;
	tone: 'info' | 'error';
}

let nextId = 1;

class Toasts {
	items = $state<Toast[]>([]);

	push(message: string, tone: Toast['tone'] = 'info') {
		const id = nextId++;
		this.items = [...this.items, { id, message, tone }];
		setTimeout(() => this.dismiss(id), tone === 'error' ? 6000 : 3000);
	}

	error(e: unknown) {
		// A 401 is not news: the layout is already sending you to the login
		// screen, and a toast on top of that just looks like a bug.
		if (e instanceof ApiError && e.isAuth) return;
		this.push(e instanceof Error ? e.message : String(e), 'error');
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
	}
}

export const toasts = new Toasts();
