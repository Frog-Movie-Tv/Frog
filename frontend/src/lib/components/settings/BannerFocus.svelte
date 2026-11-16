<script lang="ts" module>
	export type BannerShape = 'column' | 'short' | 'tall' | 'full';

	/** "x% y%" as two numbers; the middle when there is nothing. */
	export function parsePosition(value: string | null | undefined) {
		const m = value?.match(/^([\d.]+)% ([\d.]+)%$/);
		return m ? { x: Number(m[1]), y: Number(m[2]) } : { x: 50, y: 50 };
	}

	export const writePosition = (p: { x: number; y: number }) =>
		`${p.x.toFixed(1)}% ${p.y.toFixed(1)}%`;

	/**
	 * How wide the banner across the top is for how tall, on a window this
	 * wide: the profile's own heights (ProfileHeader), the bar's included,
	 * since the banner runs up under it.
	 */
	export function acrossRatio(shape: Exclude<BannerShape, 'column'>, width: number, tall: number) {
		const clamp = (lo: number, v: number, hi: number) => Math.min(hi, Math.max(lo, v));
		const bar = width <= 600 ? 72 : 84;
		const height =
			shape === 'short'
				? clamp(180, width * 0.28, 320)
				: shape === 'tall'
					? clamp(240, width * 0.44, 560)
					: Math.min(tall * 0.8, 860);
		return width / (height + bar);
	}
</script>

<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { portal } from '#lib/portal';

	interface Props {
		src: string;
		shape: BannerShape;
		/** Where it sits now, "x% y%". */
		position: string | null | undefined;
		onsave: (position: string) => void;
		onclose: () => void;
	}

	let { src, shape, position, onsave, onclose }: Props = $props();

	// Starts where it is saved; the sheet keeps its own copy until Done.
	// svelte-ignore state_referenced_locally
	let pos = $state(parsePosition(position));
	let held: { x: number; y: number; from: { x: number; y: number } } | null = null;
	let picture: HTMLImageElement | undefined = $state();

	backCloses(
		() => true,
		() => onclose()
	);

	const ratio = $derived(
		shape === 'column' ? 3 / 4 : acrossRatio(shape, window.innerWidth, window.innerHeight)
	);

	const clamp = (n: number) => Math.min(100, Math.max(0, n));

	function down(event: PointerEvent) {
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		held = { x: event.clientX, y: event.clientY, from: { ...pos } };
	}

	/** Moving the picture right shows more of its left: the position falls. */
	function move(event: PointerEvent) {
		const el = picture;
		if (!held || !el?.naturalWidth || !el.naturalHeight) return;
		const box = el.getBoundingClientRect();
		const scale = Math.max(box.width / el.naturalWidth, box.height / el.naturalHeight);
		const spareX = el.naturalWidth * scale - box.width;
		const spareY = el.naturalHeight * scale - box.height;
		pos = {
			x: spareX > 1 ? clamp(held.from.x - ((event.clientX - held.x) / spareX) * 100) : pos.x,
			y: spareY > 1 ? clamp(held.from.y - ((event.clientY - held.y) / spareY) * 100) : pos.y
		};
	}

	function up() {
		held = null;
	}

	/** The arrow keys move it too, a little at a time. */
	function key(event: KeyboardEvent) {
		const step = event.shiftKey ? 10 : 2;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, -step],
			ArrowDown: [0, step]
		};
		const m = moves[event.key];
		if (!m) return;
		event.preventDefault();
		pos = { x: clamp(pos.x + m[0]), y: clamp(pos.y + m[1]) };
	}

	function done() {
		onsave(writePosition(pos));
		onclose();
	}
</script>

<svelte:window
	onkeydown={(e) => {
		if (e.key === 'Escape') onclose();
	}}
/>

<div class="layer" use:portal>
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" onclick={onclose}></div>
	<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="focus-title">
		<div class="handle" aria-hidden="true"></div>
		<h2 id="focus-title">Choose what shows</h2>
		<p class="faint small">
			{shape === 'column'
				? 'Drag to choose the part your column shows. A phone shows it across the top.'
				: 'Drag to choose the part that shows across the top.'}
		</p>
		<div class="stage">
			<!-- A slider in two directions: the arrow keys move it as a drag does. -->
			<span
				class="frame"
				class:column={shape === 'column'}
				style:aspect-ratio={ratio}
				tabindex="0"
				role="slider"
				aria-label="Banner, drag or use the arrow keys to move it"
				aria-valuenow={Math.round(shape === 'column' ? pos.x : pos.y)}
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuetext="{Math.round(pos.x)}% across, {Math.round(pos.y)}% down"
				onpointerdown={down}
				onpointermove={move}
				onpointerup={up}
				onpointercancel={up}
				onkeydown={key}
			>
				<img
					bind:this={picture}
					{src}
					alt=""
					draggable="false"
					style:object-position="{pos.x}% {pos.y}%"
				/>
			</span>
		</div>
		<div class="actions">
			<button class="btn ghost" onclick={onclose}>Cancel</button>
			<button class="btn primary" onclick={done}>Done</button>
		</div>
	</div>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 199;
		background: rgb(0 0 0 / 0.5);
		animation: fade 160ms ease backwards;
	}

	.sheet {
		position: fixed;
		z-index: 200;
		left: 50%;
		top: 50%;
		translate: -50% -50%;
		width: min(680px, calc(100vw - 32px));
		max-height: calc(100dvh - 48px);
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 20px;
		border-radius: var(--radius-lg);
		border: 1px solid var(--line-strong);
		background: var(--bg-float);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		animation: pop 180ms var(--spring) backwards;
	}

	h2 {
		margin: 0;
		font-size: 1.15rem;
	}

	.small {
		margin: 0;
		font-size: 0.84rem;
	}

	.stage {
		display: grid;
		place-items: center;
		margin-top: 6px;
	}

	.frame {
		display: block;
		width: 100%;
		max-height: 56dvh;
		border-radius: var(--radius);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.frame.column {
		width: min(260px, 62vw);
	}

	.frame {
		cursor: grab;
		touch-action: none;
		user-select: none;
	}

	.frame:active {
		cursor: grabbing;
	}

	.frame:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.frame img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		pointer-events: none;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
		margin-top: 6px;
	}

	.handle {
		display: none;
	}

	/* A phone: a sheet along the bottom, in reach of a thumb. */
	@media (max-width: 600px) {
		.sheet {
			inset: auto 0 0 0;
			translate: none;
			width: auto;
			max-height: 90dvh;
			padding: 0 16px calc(16px + env(safe-area-inset-bottom));
			border-radius: var(--radius-lg) var(--radius-lg) 0 0;
			border-width: 1px 0 0;
			animation: rise 220ms var(--ease) backwards;
		}

		.handle {
			display: block;
			width: 42px;
			height: 4px;
			border-radius: var(--radius-pill);
			background: var(--line-strong);
			margin: 10px auto 4px;
		}

		.actions .btn {
			flex: 1;
		}
	}

	@keyframes rise {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}

	@keyframes fade {
		from {
			opacity: 0;
		}
	}
</style>
