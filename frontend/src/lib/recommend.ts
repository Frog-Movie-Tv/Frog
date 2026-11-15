import { api, frogId } from './api';
import { peoplePicker } from './people.svelte';
import type { TitleRef } from './titles';
import { toasts } from './toast.svelte';

/** Recommend a title to people on this Frog: choose who, add a note if you like, send. */
export async function recommendTitle(t: TitleRef & { name: string }) {
	try {
		const id = await frogId(t);
		const chosen = await peoplePicker.ask({
			heading: `Recommend ${t.name}`,
			purpose: 'recommend',
			titleId: id,
			note: true,
			confirm: 'Send'
		});
		if (!chosen || !chosen.ids.length) return;
		const { told } = await api.recommend(id, chosen.ids, chosen.note);
		toasts.push(told === 1 ? `Recommended ${t.name}` : `Recommended ${t.name} to ${told} people`);
	} catch (e) {
		toasts.error(e);
	}
}
