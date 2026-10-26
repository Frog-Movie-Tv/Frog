<script lang="ts">
	import { fade, fly } from 'svelte/transition';
	import Icon from './Icon.svelte';
	import Spinner from './Spinner.svelte';
	import { api } from '#lib/api';
	import { renderReview, toBlob } from '#lib/review';
	import { toasts } from '#lib/toast.svelte';
	import type { FullStats } from '#lib/types';

	interface Props {
		year: number;
		/** Already-loaded stats for this year, to skip a request. */
		stats?: FullStats | null;
		onclose: () => void;
	}

	let { year, stats = null, onclose }: Props = $props();

	let preview = $state<string | null>(null);
	let blob = $state<Blob | null>(null);
	let error = $state<string | null>(null);
	let busy = $state(false);

	const fileName = $derived(`frog-${year}-in-review.png`);
	const canShare = $derived(
		!!blob &&
			typeof navigator !== 'undefined' &&
			!!navigator.canShare?.({ files: [new File([blob], fileName, { type: 'image/png' })] })
	);

	$effect(() => {
		let cancelled = false;
		let url: string | null = null;
		(async () => {
			try {
				const data = stats?.year === year ? stats : await api.fullStats(year);
				const canvas = await renderReview(data, year);
				const b = await toBlob(canvas);
				if (cancelled) return;
				blob = b;
				url = URL.createObjectURL(b);
				preview = url;
			} catch (e) {
				if (!cancelled) error = e instanceof Error ? e.message : 'Could not create the image.';
			}
		})();
		return () => {
			cancelled = true;
			if (url) URL.revokeObjectURL(url);
		};
	});

	async function share() {
		if (!blob) return;
		busy = true;
		try {
			const file = new File([blob], fileName, { type: 'image/png' });
			await navigator.share({ files: [file], title: `${year} in review` });
		} catch (e) {
			// Closing the share sheet is not an error worth showing.
			if (!(e instanceof DOMException && e.name === 'AbortError')) save();
		} finally {
			busy = false;
		}
	}

	function save() {
		if (!preview) return;
		const a = document.createElement('a');
		a.href = preview;
		a.download = fileName;
		a.click();
		toasts.push('Image saved');
	}

	function onKey(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" transition:fade={{ duration: 120 }} onclick={onclose}></div>

<div
	class="dialog"
	role="dialog"
	aria-modal="true"
	aria-labelledby="review-title"
	transition:fly={{ y: 16, duration: 180 }}
>
	<div class="head">
		<h2 id="review-title">{year} in review</h2>
		<button class="btn ghost icon-only" aria-label="Close" onclick={onclose}>
			<Icon name="x" />
		</button>
	</div>

	<div class="frame">
		{#if error}
			<p class="muted">{error}</p>
		{:else if preview}
			<img src={preview} alt="Your {year} in film and TV" />
		{:else}
			<Spinner label="Drawing your year" />
		{/if}
	</div>

	<div class="actions">
		{#if canShare}
			<button class="btn primary" disabled={busy} onclick={share}>
				<Icon name="share" size={17} /> Share
			</button>
			<button class="btn" disabled={!preview} onclick={save}>
				<Icon name="download" size={17} /> Save
			</button>
		{:else}
			<button class="btn primary" disabled={!preview} onclick={save}>
				<Icon name="download" size={17} /> Save image
			</button>
		{/if}
	</div>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 95;
		background: rgba(0, 0, 0, 0.6);
	}

	.dialog {
		position: fixed;
		z-index: 96;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		width: min(420px, calc(100vw - 28px));
		max-height: calc(100dvh - 32px);
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 18px;
		border-radius: var(--radius-lg);
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		font-size: 1.15rem;
	}

	.icon-only {
		padding: 8px;
	}

	.frame {
		flex: 1;
		min-height: 0;
		display: grid;
		place-items: center;
		aspect-ratio: 9 / 16;
		max-height: calc(100dvh - 170px);
		margin: 0 auto;
		width: 100%;
		border-radius: var(--radius);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.frame img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}

	.actions {
		display: flex;
		gap: 10px;
		justify-content: flex-end;
	}

	.actions .btn {
		flex: 1;
	}
</style>
