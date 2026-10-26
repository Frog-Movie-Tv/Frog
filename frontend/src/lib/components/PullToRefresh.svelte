<script lang="ts">
	import { onMount } from 'svelte';
	import { settings } from '#lib/settings.svelte';

	/**
	 * Pull down from the top of any page to reload it, on touch screens.
	 *
	 * An installed app has no browser pull-to-refresh at all, and in a browser
	 * tab Chrome's own one fights the page's scrolling. So on touch screens the
	 * native one is switched off (overscroll-behavior) and this takes its place:
	 * the same gesture everywhere, with the mascot as the indicator.
	 */

	/** How far, in pixels of finger travel, before letting go reloads. */
	const THRESHOLD = 110;

	let pull = $state(0);
	let reloading = $state(false);
	let startY: number | null = null;

	/** A gesture that starts inside something scrolled, or a menu, is not ours. */
	function startsInScroller(target: EventTarget | null): boolean {
		let el = target instanceof Element ? target : null;
		while (el && el !== document.body) {
			if (el.closest('[role="dialog"], [role="menu"], .more-sheet')) return true;
			if (el.scrollTop > 0) return true;
			el = el.parentElement;
		}
		return false;
	}

	onMount(() => {
		if (!matchMedia('(pointer: coarse)').matches) return;
		const root = document.documentElement;
		const before = root.style.overscrollBehaviorY;
		root.style.overscrollBehaviorY = 'contain';

		const down = (e: TouchEvent) => {
			if (reloading || e.touches.length !== 1) return;
			if (window.scrollY > 0 || startsInScroller(e.target)) {
				startY = null;
				return;
			}
			startY = e.touches[0].clientY;
		};
		const move = (e: TouchEvent) => {
			if (startY === null) return;
			const dy = e.touches[0].clientY - startY;
			if (dy <= 0 || window.scrollY > 0) {
				pull = 0;
				return;
			}
			pull = dy;
		};
		const up = () => {
			if (startY === null) return;
			startY = null;
			if (pull >= THRESHOLD) {
				reloading = true;
				location.reload();
			} else {
				pull = 0;
			}
		};

		addEventListener('touchstart', down, { passive: true });
		addEventListener('touchmove', move, { passive: true });
		addEventListener('touchend', up);
		addEventListener('touchcancel', up);
		return () => {
			root.style.overscrollBehaviorY = before;
			removeEventListener('touchstart', down);
			removeEventListener('touchmove', move);
			removeEventListener('touchend', up);
			removeEventListener('touchcancel', up);
		};
	});

	// Resistance: the indicator follows the finger at a falling rate.
	const offset = $derived(Math.min(80, pull * 0.45));
	const ready = $derived(pull >= THRESHOLD);
	const progress = $derived(Math.min(1, pull / THRESHOLD));
</script>

{#if pull > 8 || reloading}
	<div
		class="ptr"
		class:ready
		class:reloading
		style:transform="translate(-50%, {reloading ? 64 : offset}px)"
		role="status"
		aria-live="polite"
	>
		<img
			src="/mascots/{settings.current.mascot}.svg"
			alt=""
			width="30"
			height="30"
			style:transform="rotate({progress * 360}deg)"
		/>
		<span class="sr"
			>{reloading ? 'Refreshing' : ready ? 'Release to refresh' : 'Pull to refresh'}</span
		>
		<svg class="ring" viewBox="0 0 44 44" aria-hidden="true">
			<circle cx="22" cy="22" r="20" style:stroke-dashoffset={125.6 * (1 - progress)} />
		</svg>
	</div>
{/if}

<style>
	.ptr {
		position: fixed;
		top: calc(var(--header-h) - 34px);
		left: 50%;
		z-index: 70;
		width: 44px;
		height: 44px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		box-shadow: var(--shadow-3);
		pointer-events: none;
	}

	.ptr img {
		grid-area: 1 / 1;
	}

	.ring {
		grid-area: 1 / 1;
		width: 44px;
		height: 44px;
		transform: rotate(-90deg);
	}

	.ring circle {
		fill: none;
		stroke: var(--accent);
		stroke-width: 3;
		stroke-dasharray: 125.6;
		stroke-linecap: round;
		opacity: 0.6;
	}

	.ready .ring circle {
		opacity: 1;
	}

	.reloading img {
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
