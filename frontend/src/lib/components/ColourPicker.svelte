<script lang="ts" module>
	/** A few colours that read well on every appearance, light or dark. */
	export const SWATCHES = [
		'#e5484d',
		'#f76b15',
		'#ffc53d',
		'#a2c93a',
		'#46a758',
		'#12a594',
		'#00a2c7',
		'#3e63dd',
		'#6e56cf',
		'#ab4aba',
		'#d6409f',
		'#8d8d86'
	];

	/** #rrggbb to hue (0–360), saturation and value (0–1). */
	export function toHsv(hex: string): [number, number, number] {
		const n = parseInt(hex.slice(1), 16);
		const r = ((n >> 16) & 255) / 255;
		const g = ((n >> 8) & 255) / 255;
		const b = (n & 255) / 255;
		const max = Math.max(r, g, b);
		const d = max - Math.min(r, g, b);
		let h = 0;
		if (d) {
			if (max === r) h = ((g - b) / d) % 6;
			else if (max === g) h = (b - r) / d + 2;
			else h = (r - g) / d + 4;
			h = (h * 60 + 360) % 360;
		}
		return [h, max ? d / max : 0, max];
	}

	export function toHex(h: number, s: number, v: number): string {
		const f = (n: number) => {
			const k = (n + h / 60) % 6;
			const c = v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
			return Math.round(c * 255)
				.toString(16)
				.padStart(2, '0');
		};
		return `#${f(5)}${f(3)}${f(1)}`;
	}

	export const isHex = (s: string) => /^#[0-9a-f]{6}$/i.test(s);
</script>

<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { onScreen } from '#lib/onscreen';
	import Icon from './Icon.svelte';

	interface Props {
		value: string;
		label: string;
		/** Each change as it happens, while dragging too. */
		oninput?: (hex: string) => void;
		/** A choice settled: a swatch, the end of a drag, a typed colour. */
		onchange?: (hex: string) => void;
	}

	let { value, label, oninput, onchange }: Props = $props();

	let open = $state(false);
	let root: HTMLDivElement | undefined = $state();
	let hsv = $state<[number, number, number]>([0, 0, 0]);
	let typed = $state('');

	const hex = $derived(toHex(...hsv));

	backCloses(
		() => open,
		() => close()
	);

	function toggle() {
		if (open) return close();
		hsv = toHsv(isHex(value) ? value : '#888888');
		typed = hex;
		open = true;
	}

	function close() {
		if (!open) return;
		open = false;
		if (hex !== value.toLowerCase()) onchange?.(hex);
	}

	function set(next: [number, number, number], settle = false) {
		hsv = next;
		typed = hex;
		oninput?.(hex);
		if (settle) onchange?.(hex);
	}

	function pick(colour: string) {
		set(toHsv(colour), true);
	}

	/** The shade pad: across is saturation, up is brightness. */
	function pad(node: HTMLElement) {
		const at = (e: PointerEvent) => {
			const r = node.getBoundingClientRect();
			const s = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
			const v = 1 - Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
			set([hsv[0], s, v]);
		};
		const down = (e: PointerEvent) => {
			node.setPointerCapture(e.pointerId);
			at(e);
		};
		const move = (e: PointerEvent) => {
			if (node.hasPointerCapture(e.pointerId)) at(e);
		};
		const up = (e: PointerEvent) => {
			if (node.hasPointerCapture(e.pointerId)) onchange?.(hex);
		};
		node.addEventListener('pointerdown', down);
		node.addEventListener('pointermove', move);
		node.addEventListener('pointerup', up);
		return {
			destroy() {
				node.removeEventListener('pointerdown', down);
				node.removeEventListener('pointermove', move);
				node.removeEventListener('pointerup', up);
			}
		};
	}

	function padKeys(e: KeyboardEvent) {
		const step = e.shiftKey ? 0.1 : 0.02;
		const [h, s, v] = hsv;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, step],
			ArrowDown: [0, -step]
		};
		const m = moves[e.key];
		if (!m) return;
		e.preventDefault();
		const clamp = (n: number) => Math.min(1, Math.max(0, n));
		set([h, clamp(s + m[0]), clamp(v + m[1])], true);
	}

	function typedIn() {
		const t = typed.trim().startsWith('#') ? typed.trim() : `#${typed.trim()}`;
		if (isHex(t)) pick(t.toLowerCase());
		else typed = hex;
	}
</script>

<svelte:window
	onpointerdown={(e) => {
		if (open && root && !root.contains(e.target as Node)) close();
	}}
	onkeydown={(e) => {
		if (open && e.key === 'Escape') close();
	}}
/>

<div class="picker" bind:this={root}>
	<button
		type="button"
		class="trigger"
		aria-label={label}
		aria-haspopup="dialog"
		aria-expanded={open}
		title={label}
		style:--chosen={open ? hex : value}
		onclick={toggle}
	></button>

	{#if open}
		<div class="scrim" aria-hidden="true" onpointerdown={close}></div>
		<div class="panel" role="dialog" aria-label={label} use:onScreen>
			<div class="swatches">
				{#each SWATCHES as colour (colour)}
					<button
						type="button"
						class="swatch"
						class:on={colour === hex}
						style:--c={colour}
						aria-label={colour}
						aria-pressed={colour === hex}
						onclick={() => pick(colour)}
					>
						{#if colour === hex}<Icon name="check" size={14} />{/if}
					</button>
				{/each}
			</div>

			<div
				class="pad"
				role="slider"
				tabindex="0"
				aria-label="Shade"
				aria-valuetext="{Math.round(hsv[1] * 100)}% colour, {Math.round(hsv[2] * 100)}% light"
				aria-valuenow={Math.round(hsv[2] * 100)}
				style:--hue={hsv[0]}
				use:pad
				onkeydown={padKeys}
			>
				<span
					class="knob"
					style:left="{hsv[1] * 100}%"
					style:top="{(1 - hsv[2]) * 100}%"
					style:background={hex}
				></span>
			</div>

			<input
				class="hue"
				type="range"
				min="0"
				max="359"
				aria-label="Hue"
				value={Math.round(hsv[0])}
				style:--hue={hsv[0]}
				oninput={(e) => set([Number(e.currentTarget.value), hsv[1] || 0.7, hsv[2] || 0.8])}
				onchange={() => onchange?.(hex)}
			/>

			<div class="foot">
				<span class="now" style:background={hex} aria-hidden="true"></span>
				<input
					class="input hex"
					aria-label="Hex colour"
					maxlength="7"
					spellcheck="false"
					bind:value={typed}
					onchange={typedIn}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault();
							typedIn();
						}
					}}
				/>
				<button type="button" class="btn primary small" onclick={close}>Done</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.picker {
		position: relative;
		display: inline-flex;
		flex: none;
	}

	.trigger {
		width: 32px;
		height: 32px;
		padding: 0;
		border-radius: 50%;
		border: 2px solid var(--bg-float);
		background: var(--chosen);
		box-shadow:
			0 0 0 1px var(--line-strong),
			var(--shadow-1, 0 1px 3px rgb(0 0 0 / 0.2));
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.trigger:hover {
		transform: scale(1.08);
	}

	.trigger:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.panel {
		position: absolute;
		z-index: 70;
		top: calc(100% + 8px);
		right: 0;
		width: 248px;
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 14px;
		border-radius: var(--radius);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
		animation: pop 180ms var(--spring) backwards;
	}

	.swatches {
		display: grid;
		grid-template-columns: repeat(6, 1fr);
		gap: 8px;
	}

	.swatch {
		display: grid;
		place-items: center;
		aspect-ratio: 1;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: var(--c);
		color: #fff;
		cursor: pointer;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
		transition: transform var(--fast) var(--ease);
	}

	.swatch:hover {
		transform: scale(1.12);
	}

	.swatch.on {
		box-shadow:
			0 0 0 2px var(--bg-float),
			0 0 0 4px var(--c);
	}

	.pad {
		position: relative;
		height: 132px;
		border-radius: var(--radius-sm);
		background:
			linear-gradient(to top, #000, transparent),
			linear-gradient(to right, #fff, hsl(var(--hue) 100% 50%));
		cursor: crosshair;
		touch-action: none;
	}

	.pad:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.knob {
		position: absolute;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 2.5px solid #fff;
		box-shadow: 0 1px 4px rgb(0 0 0 / 0.4);
		translate: -50% -50%;
		pointer-events: none;
	}

	.hue {
		appearance: none;
		width: 100%;
		height: 14px;
		margin: 0;
		border-radius: 999px;
		background: linear-gradient(
			to right,
			#f00 0%,
			#ff0 17%,
			#0f0 33%,
			#0ff 50%,
			#00f 67%,
			#f0f 83%,
			#f00 100%
		);
		cursor: pointer;
		touch-action: none;
	}

	.hue::-webkit-slider-thumb {
		appearance: none;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		border: 2.5px solid #fff;
		background: hsl(var(--hue) 100% 50%);
		box-shadow: 0 1px 4px rgb(0 0 0 / 0.4);
	}

	.hue::-moz-range-thumb {
		width: 15px;
		height: 15px;
		border-radius: 50%;
		border: 2.5px solid #fff;
		background: hsl(var(--hue) 100% 50%);
		box-shadow: 0 1px 4px rgb(0 0 0 / 0.4);
	}

	.foot {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.now {
		flex: none;
		width: 28px;
		height: 28px;
		border-radius: 8px;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.1);
	}

	.hex {
		flex: 1;
		min-width: 0;
		padding: 6px 9px;
		font-family: var(--font-mono);
		font-size: 0.82rem;
		text-transform: lowercase;
	}

	.scrim {
		display: none;
	}

	/* A phone: a sheet from the bottom, with room for a thumb. */
	@media (max-width: 600px) {
		.scrim {
			display: block;
			position: fixed;
			inset: 0;
			z-index: 199;
			background: rgb(0 0 0 / 0.45);
		}

		.panel {
			position: fixed;
			z-index: 200;
			top: auto;
			left: 10px;
			right: 10px;
			bottom: calc(10px + env(safe-area-inset-bottom));
			width: auto;
			padding: 18px;
			gap: 16px;
			border-radius: 18px;
			animation: sheet-up 220ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
		}

		.swatches {
			gap: 12px;
		}

		.pad {
			height: 168px;
		}

		.hue {
			height: 22px;
		}

		.hue::-webkit-slider-thumb {
			width: 28px;
			height: 28px;
		}
	}

	@keyframes sheet-up {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}
</style>
