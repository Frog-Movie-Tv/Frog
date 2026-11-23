<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** A smart shelf: a slow light runs along its edge. */
		sheen?: boolean;
		/** What the front edge carries: a shelf's label, or a card per title. */
		children?: Snippet;
	}

	let { sheen = false, children }: Props = $props();
</script>

<!-- A glass shelf seen from a little above: its top catches the light, and
     its front edge is deep enough to carry a label. Its light comes from
     `--glow` on whatever holds it. -->
<div class="plank" class:sheen>
	<div class="top"></div>
	<div class="front">{@render children?.()}</div>
</div>

<style>
	.plank {
		position: relative;
		margin: 0 -10px;
	}

	.top {
		position: relative;
		height: 9px;
		border-radius: 5px 5px 0 0;
		background: linear-gradient(180deg, var(--line-strong), var(--line));
		box-shadow: inset 0 1px 0 var(--shine);
	}

	.top::after {
		content: '';
		position: absolute;
		left: 4%;
		right: 4%;
		bottom: 0;
		height: 2px;
		border-radius: 2px;
		background: linear-gradient(90deg, transparent, var(--glow, var(--accent)), transparent);
		opacity: var(--edge-light, 0.22);
		transition: opacity 450ms var(--ease);
	}

	.sheen .top::after {
		background: linear-gradient(
			90deg,
			transparent 0 35%,
			var(--glow, var(--accent)) 50%,
			transparent 65% 100%
		);
		background-size: 300% 100%;
		animation: sheen 4.5s linear infinite;
		opacity: max(0.55, var(--edge-light, 0.22));
	}

	@keyframes sheen {
		from {
			background-position: 100% 0;
		}
		to {
			background-position: -100% 0;
		}
	}

	.front {
		position: relative;
		display: flex;
		align-items: center;
		gap: 14px;
		min-height: 46px;
		padding: 6px 14px;
		border-radius: 0 0 10px 10px;
		background: linear-gradient(180deg, var(--bg-elev-2), var(--bg-elev));
		box-shadow:
			inset 0 1px 0 color-mix(in srgb, var(--bg) 40%, transparent),
			inset 0 -1px 0 var(--shine),
			0 26px 30px -18px color-mix(in srgb, var(--bg) 85%, transparent);
	}

	/* Light spilling down from under the shelf. */
	.plank::after {
		content: '';
		position: absolute;
		left: 10%;
		right: 10%;
		top: 100%;
		height: 40px;
		background: radial-gradient(
			50% 100% at 50% 0,
			color-mix(in srgb, var(--glow, var(--accent)) 16%, transparent),
			transparent 80%
		);
		pointer-events: none;
		opacity: var(--spill, 0.6);
		transition: opacity 450ms var(--ease);
	}

	@media (max-width: 720px) {
		.front {
			flex-wrap: wrap;
			gap: 6px 10px;
			padding: 7px 10px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.sheen .top::after {
			animation: none;
		}
	}
</style>
