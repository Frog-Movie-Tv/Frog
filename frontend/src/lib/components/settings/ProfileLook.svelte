<script lang="ts">
	import {
		COLOUR_NAMES,
		PALETTE,
		PROFILE_COLOURS,
		PROFILE_SHADES,
		SHADE_NAMES,
		isColour,
		isShade,
		profileStyle,
		type ProfileShade
	} from '#lib/profile-colours';
	import { currentTone } from '#lib/settings.svelte';
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
	const preview = $derived(profileStyle(colour, shade, tone));
	const name = $derived(account?.display_name || account?.username || '');
	const layout = $derived(account?.banner_layout ?? 'classic');
	const height = $derived(account?.banner_height ?? 'short');
</script>

<div class="look">
	<div class="head">
		<strong>Colour</strong>
		<span class="faint">Your profile wears it for everyone who opens it.</span>
	</div>

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
			></button>
		{/each}
	</div>

	{#if colour}
		<div class="segmented shades" role="radiogroup" aria-label="Shade">
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

	<!-- Your profile in miniature, in the colour picked. -->
	<div class="preview" style={preview} aria-hidden="true">
		<span class="banner"></span>
		<span class="row">
			<span class="face">{name.slice(0, 1).toUpperCase()}</span>
			<span class="who">
				<b>{name}</b>
				<span class="taste"><strong>87%</strong> taste match</span>
			</span>
			<span class="button">Compare</span>
		</span>
		<span class="tiles">
			<span class="tile"><span>Films</span><b>42</b></span>
			<span class="tile"><span>Episodes</span><b>318</b></span>
			<span class="tile"><span>Streak</span><b>6 days</b></span>
		</span>
	</div>

	<div class="head">
		<strong>Banner</strong>
		<span class="faint">
			{layout === 'vertical'
				? 'A portrait column beside everything else, and tall across the top on a phone.'
				: 'Across the top of your profile. Drag the picture above to choose what shows.'}
			A tap on it opens it whole.
		</span>
	</div>
	<div class="choices">
		<div class="segmented" role="radiogroup" aria-label="Banner layout">
			{#each [['classic', 'Classic'], ['vertical', 'Vertical']] as [id, label] (id)}
				<button
					class:on={layout === id}
					role="radio"
					aria-checked={layout === id}
					onclick={() => save({ banner_layout: id === 'classic' ? '' : id })}>{label}</button
				>
			{/each}
		</div>
		{#if layout !== 'vertical'}
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
		{/if}
	</div>
	{#if layout === 'vertical' && banner}
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
				Drag to choose what the column shows. A phone shows the same part, across the top.
			</span>
		</div>
	{/if}
</div>

<style>
	.look {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px 20px;
		border-top: 1px solid var(--line);
	}

	.head {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.head .faint {
		font-size: 0.84rem;
	}

	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
		cursor: pointer;
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

	.shades {
		align-self: flex-start;
	}

	.column {
		display: flex;
		align-items: center;
		gap: 14px;
	}

	.portrait {
		flex: none;
		width: 108px;
		aspect-ratio: 3 / 4;
		border-radius: 12px;
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

	.choices {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.preview {
		display: flex;
		flex-direction: column;
		max-width: 420px;
		border-radius: var(--radius);
		overflow: hidden;
		border: 1px solid var(--line);
		background: var(--bg);
	}

	.banner {
		height: 64px;
		background:
			radial-gradient(circle at 20% 20%, var(--accent-soft), transparent 55%),
			linear-gradient(135deg, var(--bg-elev-2), var(--bg) 80%);
	}

	.row {
		display: flex;
		align-items: flex-end;
		gap: 10px;
		margin-top: -22px;
		padding: 0 12px;
		min-width: 0;
	}

	.face {
		flex: none;
		display: grid;
		place-items: center;
		width: 44px;
		height: 44px;
		border-radius: 50%;
		border: 3px solid var(--bg);
		box-shadow: 0 0 0 2px var(--accent);
		background: var(--accent-soft);
		color: var(--accent);
		font-weight: 700;
	}

	.who {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		font-size: 0.82rem;
	}

	.who b {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.taste {
		color: var(--text-dim);
		font-size: 0.74rem;
	}

	.taste strong {
		color: var(--accent);
	}

	.button {
		flex: none;
		padding: 4px 10px;
		border-radius: 999px;
		background: var(--accent);
		color: var(--accent-ink);
		font-size: 0.74rem;
		font-weight: 650;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 6px;
		padding: 10px 12px 12px;
	}

	.tile {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 8px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-elev));
		font-size: 0.7rem;
		min-width: 0;
	}

	.tile span {
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-size: 0.62rem;
		font-weight: 650;
	}
</style>
