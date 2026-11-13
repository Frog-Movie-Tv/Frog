<script lang="ts">
	import {
		COLOUR_NAMES,
		PALETTE,
		PROFILE_COLOURS,
		PROFILE_SHADES,
		SHADE_NAMES,
		isColour,
		isShade,
		type ProfileShade
	} from '#lib/profile-colours';
	import { currentTone } from '#lib/settings.svelte';
	import Icon from '../Icon.svelte';
	import type { Account, ProfileEdit } from '#lib/types';

	interface Props {
		account: Account | null;
		save: (edit: ProfileEdit, message?: string) => Promise<void>;
		/** The banner's picture, when there is one. */
		banner?: string | null;
	}

	let { account, save, banner = null }: Props = $props();

	// The vertical column, and what a phone shows of it, in a portrait frame
	// of its own. The same position as the wide banner, so moving either
	// moves both.
	const position = (value: string | null | undefined) => {
		const m = value?.match(/^([\d.]+)% ([\d.]+)%$/);
		return m ? { x: Number(m[1]), y: Number(m[2]) } : { x: 50, y: 50 };
	};
	let pos = $derived(position(account?.banner_position));
	let held: { x: number; y: number; from: { x: number; y: number } } | null = null;

	function down(event: PointerEvent) {
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		held = { x: event.clientX, y: event.clientY, from: { ...pos } };
	}

	function move(event: PointerEvent) {
		if (!held) return;
		const el = event.currentTarget as HTMLImageElement;
		const box = el.getBoundingClientRect();
		const scale = Math.max(box.width / el.naturalWidth, box.height / el.naturalHeight);
		const spareX = el.naturalWidth * scale - box.width;
		const spareY = el.naturalHeight * scale - box.height;
		const clamp = (n: number) => Math.min(100, Math.max(0, n));
		pos = {
			x: spareX > 1 ? clamp(held.from.x - ((event.clientX - held.x) / spareX) * 100) : pos.x,
			y: spareY > 1 ? clamp(held.from.y - ((event.clientY - held.y) / spareY) * 100) : pos.y
		};
	}

	function up() {
		if (!held) return;
		held = null;
		void save(
			{ banner_position: `${pos.x.toFixed(1)}% ${pos.y.toFixed(1)}%` },
			'Banner position saved'
		);
	}

	const colour = $derived.by(() => {
		const c = account?.profile_color;
		return isColour(c) ? c : null;
	});
	const shade = $derived.by((): ProfileShade => {
		const s = account?.profile_shade;
		return isShade(s) ? s : 'medium';
	});
	const tone = $derived(currentTone());
	const layout = $derived(account?.banner_layout ?? 'classic');
	const height = $derived(account?.banner_height ?? 'short');
</script>

<div class="setting">
	<div class="setting-top">
		<span class="setting-tile" aria-hidden="true"><Icon name="palette" size={18} /></span>
		<span class="setting-label">
			<strong>Colour</strong>
			<span class="faint">Your profile wears it for everyone who opens it.</span>
		</span>
	</div>
	<div class="setting-body colours">
		<div class="swatches" role="radiogroup" aria-label="Profile colour">
			<button
				class="swatch none"
				class:on={!colour}
				role="radio"
				aria-checked={!colour}
				aria-label="The theme's own"
				title="The theme's own"
				onclick={() => save({ profile_color: '' })}
			></button>
			{#each PROFILE_COLOURS as c (c)}
				<button
					class="swatch"
					class:on={colour === c}
					role="radio"
					aria-checked={colour === c}
					aria-label={COLOUR_NAMES[c]}
					title={COLOUR_NAMES[c]}
					style:background={PALETTE[c][shade][tone]}
					onclick={() => save({ profile_color: c, profile_shade: shade })}
				>
					{#if colour === c}<Icon name="check" size={14} />{/if}
				</button>
			{/each}
		</div>
		{#if colour}
			<div class="segmented" role="radiogroup" aria-label="Shade">
				{#each PROFILE_SHADES as s (s)}
					<button
						class:on={shade === s}
						role="radio"
						aria-checked={shade === s}
						onclick={() => save({ profile_shade: s })}>{SHADE_NAMES[s]}</button
					>
				{/each}
			</div>
		{/if}
	</div>
</div>

<div class="setting">
	<div class="setting-top">
		<span class="setting-tile" aria-hidden="true"><Icon name="layers" size={18} /></span>
		<span class="setting-label">
			<strong>Banner</strong>
			<span class="faint">How your banner sits on your profile. A tap on it opens it whole.</span>
		</span>
	</div>
	<div class="setting-body banner-choices">
		<div class="layouts" role="radiogroup" aria-label="Banner layout">
			{#each [['classic', 'Across the top'], ['vertical', 'A column beside it']] as [id, label] (id)}
				<button
					class="layout"
					class:on={layout === id}
					role="radio"
					aria-checked={layout === id}
					onclick={() => save({ banner_layout: id === 'classic' ? '' : id })}
				>
					<span class="sketch {id}" aria-hidden="true">
						<i class="pic"></i><i class="line"></i><i class="line short"></i><i class="tiles"></i>
					</span>
					<span>{label}</span>
				</button>
			{/each}
		</div>

		{#if layout === 'vertical'}
			{#if banner}
				<div class="column">
					<span class="portrait">
						<img
							src={banner}
							alt=""
							draggable="false"
							style:object-position="{pos.x}% {pos.y}%"
							onpointerdown={down}
							onpointermove={move}
							onpointerup={up}
							onpointercancel={up}
						/>
					</span>
					<span class="faint small">
						Drag to choose what the column shows. A phone shows the same part across the top.
					</span>
				</div>
			{/if}
		{:else}
			<div class="height">
				<span class="small">Height</span>
				<div class="segmented" role="radiogroup" aria-label="Banner height">
					{#each [['short', 'Short'], ['tall', 'Tall'], ['full', 'Full']] as [id, label] (id)}
						<button
							class:on={height === id}
							role="radio"
							aria-checked={height === id}
							onclick={() => save({ banner_height: id === 'short' ? '' : id })}>{label}</button
						>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.colours,
	.banner-choices {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 12px;
	}

	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		color: #fff;
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.swatch:hover {
		transform: scale(1.08);
	}

	.swatch.none {
		background:
			linear-gradient(
				135deg,
				transparent 45%,
				var(--line-strong) 45%,
				var(--line-strong) 55%,
				transparent 55%
			),
			var(--bg-elev);
	}

	.swatch.on {
		outline: 2px solid var(--text);
		outline-offset: 2px;
	}

	/* Each layout as a little sketch of the page it makes. */
	.layouts {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}

	.layout {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 10px 10px 8px;
		border-radius: var(--radius-sm);
		border: 1.5px solid var(--line);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font: inherit;
		font-size: 0.82rem;
		font-weight: 600;
		cursor: pointer;
	}

	.layout.on {
		border-color: var(--accent);
		color: var(--text);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}

	.sketch {
		position: relative;
		display: block;
		width: 112px;
		height: 72px;
		border-radius: 6px;
		background: var(--bg);
		overflow: hidden;
	}

	.sketch i {
		position: absolute;
		display: block;
		border-radius: 2px;
		background: var(--line-strong);
	}

	.sketch .pic {
		background: color-mix(in srgb, var(--accent) 55%, var(--bg-elev-2));
	}

	.sketch.classic .pic {
		inset: 0 0 auto 0;
		height: 26px;
		border-radius: 0;
	}

	.sketch.classic .line {
		left: 8px;
		top: 32px;
		width: 50px;
		height: 5px;
	}

	.sketch.classic .line.short {
		top: 41px;
		width: 32px;
	}

	.sketch.classic .tiles {
		left: 8px;
		right: 8px;
		bottom: 8px;
		height: 12px;
	}

	.sketch.vertical .pic {
		left: 6px;
		top: 6px;
		bottom: 6px;
		width: 34px;
	}

	.sketch.vertical .line {
		left: 48px;
		top: 10px;
		width: 44px;
		height: 5px;
	}

	.sketch.vertical .line.short {
		top: 19px;
		width: 30px;
	}

	.sketch.vertical .tiles {
		left: 48px;
		right: 8px;
		top: 32px;
		bottom: 8px;
	}

	.height {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.column {
		display: flex;
		align-items: center;
		gap: 14px;
	}

	.portrait {
		flex: none;
		width: 96px;
		aspect-ratio: 3 / 4;
		border-radius: 10px;
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.portrait img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		cursor: grab;
		touch-action: none;
		user-select: none;
	}

	.portrait img:active {
		cursor: grabbing;
	}

	.small {
		font-size: 0.82rem;
	}
</style>
