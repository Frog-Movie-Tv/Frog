<script lang="ts">
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import { session } from '#lib/session.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { Account, LibraryItem, ProfileEdit } from '#lib/types';

	const BIO_MAX = 600;
	const PINS_MAX = 4;

	let account = $state<Account | null>(null);
	let titles = $state<LibraryItem[]>([]);
	let name = $state('');
	let bio = $state('');
	let busy = $state(false);
	/** Bumped after an upload so the previews load the new picture. */
	let version = $state(Date.now());
	let choosingBanner = $state(false);

	// --- where the banner sits -------------------------------------------
	const parsePosition = (value: string | null | undefined) => {
		const m = value?.match(/^([\d.]+)% ([\d.]+)%$/);
		return m ? { x: Number(m[1]), y: Number(m[2]) } : { x: 50, y: 50 };
	};
	// Follows what is saved, and is written over while a drag is under way.
	let bannerPos = $derived(parsePosition(account?.banner_position));
	let dragging: { x: number; y: number; from: { x: number; y: number } } | null = null;

	function bannerDown(event: PointerEvent) {
		const el = event.currentTarget as HTMLImageElement;
		el.setPointerCapture(event.pointerId);
		dragging = { x: event.clientX, y: event.clientY, from: { ...bannerPos } };
	}

	/** Moving the picture right shows more of its left: the position falls. */
	function bannerMove(event: PointerEvent) {
		if (!dragging) return;
		const el = event.currentTarget as HTMLImageElement;
		const box = el.getBoundingClientRect();
		const scale = Math.max(box.width / el.naturalWidth, box.height / el.naturalHeight);
		const spareX = el.naturalWidth * scale - box.width;
		const spareY = el.naturalHeight * scale - box.height;
		const clamp = (n: number) => Math.min(100, Math.max(0, n));
		bannerPos = {
			x: spareX > 1 ? clamp(dragging.from.x - ((event.clientX - dragging.x) / spareX) * 100) : 50,
			y: spareY > 1 ? clamp(dragging.from.y - ((event.clientY - dragging.y) / spareY) * 100) : 50
		};
	}

	function bannerUp() {
		if (!dragging) return;
		dragging = null;
		void save(
			{ banner_position: `${bannerPos.x.toFixed(1)}% ${bannerPos.y.toFixed(1)}%` },
			'Banner position saved'
		);
	}

	// --- framing a new profile picture ------------------------------------
	const FRAME = 200;
	let framing = $state<{ url: string; width: number; height: number } | null>(null);
	let zoom = $state(1);
	let offset = $state({ x: 0, y: 0 });
	let pan: { x: number; y: number; from: { x: number; y: number } } | null = null;
	const cover = $derived(framing ? FRAME / Math.min(framing.width, framing.height) : 1);
	const drawn = $derived(
		framing
			? { w: framing.width * cover * zoom, h: framing.height * cover * zoom }
			: { w: FRAME, h: FRAME }
	);

	/** Keep the circle covered: the picture never slides off an edge. */
	function within(x: number, y: number) {
		const maxX = (drawn.w - FRAME) / 2;
		const maxY = (drawn.h - FRAME) / 2;
		return {
			x: Math.min(maxX, Math.max(-maxX, x)),
			y: Math.min(maxY, Math.max(-maxY, y))
		};
	}

	async function frame(file: File | undefined) {
		if (!file) return;
		// A moving GIF cannot be cropped on a canvas without stopping it.
		if (file.type === 'image/gif') return upload('avatar', file);
		const bitmap = await createImageBitmap(file);
		framing = { url: URL.createObjectURL(file), width: bitmap.width, height: bitmap.height };
		zoom = 1;
		offset = { x: 0, y: 0 };
	}

	function closeFrame() {
		if (framing) URL.revokeObjectURL(framing.url);
		framing = null;
	}

	async function saveFrame() {
		if (!framing) return;
		const image = new Image();
		image.src = framing.url;
		await image.decode();
		const size = 512;
		const k = size / FRAME;
		const canvas = document.createElement('canvas');
		canvas.width = canvas.height = size;
		canvas
			.getContext('2d')
			?.drawImage(
				image,
				((FRAME - drawn.w) / 2 + offset.x) * k,
				((FRAME - drawn.h) / 2 + offset.y) * k,
				drawn.w * k,
				drawn.h * k
			);
		const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/jpeg', 0.9));
		closeFrame();
		if (blob) await upload('avatar', new File([blob], 'avatar.jpg', { type: 'image/jpeg' }));
	}
	let choosingPins = $state(false);

	const username = $derived(account?.username ?? session.status?.username ?? '');
	const imageUrl = (kind: 'avatar' | 'banner') =>
		`/api/profiles/${encodeURIComponent(username)}/images/${kind}?v=${version}`;
	const pinned = $derived(
		(account?.pins ?? [])
			.map((p) => titles.find((t) => t.id === p.title_id))
			.filter((t): t is LibraryItem => !!t)
	);
	const backdrops = $derived(titles.filter((t) => t.backdrop_path).slice(0, 18));

	async function load() {
		try {
			account = await api.account();
			name = account.display_name ?? '';
			bio = account.bio ?? '';
			titles = (await api.library({ sort: 'rating', order: 'desc', limit: 60 })).items;
		} catch (e) {
			toasts.error(e);
		}
	}
	load();

	async function save(edit: ProfileEdit, message?: string) {
		busy = true;
		try {
			account = await api.updateProfile(edit);
			if (message) toasts.push(message);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	/**
	 * Phone photos are far bigger than a profile needs, so pictures are scaled
	 * down here first: quicker to send, and well under the size limit.
	 */
	async function shrink(file: File, longest: number): Promise<Blob> {
		if (file.type === 'image/gif') return file;
		const bitmap = await createImageBitmap(file);
		const scale = Math.min(1, longest / Math.max(bitmap.width, bitmap.height));
		const canvas = document.createElement('canvas');
		canvas.width = Math.round(bitmap.width * scale);
		canvas.height = Math.round(bitmap.height * scale);
		canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
		return new Promise((resolve, reject) =>
			canvas.toBlob(
				(blob) => (blob ? resolve(blob) : reject(new Error('Could not read that picture'))),
				'image/jpeg',
				0.86
			)
		);
	}

	async function upload(kind: 'avatar' | 'banner', file: File | undefined) {
		if (!file) return;
		busy = true;
		try {
			const blob = await shrink(file, kind === 'avatar' ? 512 : 1800);
			await api.uploadProfileImage(kind, blob);
			version = Date.now();
			account = await api.account();
			await session.refresh();
			toasts.push(kind === 'avatar' ? 'Profile picture updated' : 'Banner updated');
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function removeImage(kind: 'avatar' | 'banner') {
		try {
			// Hold on to the picture, so undo puts back the same one.
			const kept = await fetch(imageUrl(kind)).then((r) => (r.ok ? r.blob() : null));
			await api.removeProfileImage(kind);
			account = await api.account();
			await session.refresh();
			const what = kind === 'avatar' ? 'Profile picture' : 'Banner';
			if (!kept) {
				toasts.push(`${what} removed`);
				return;
			}
			toasts.undoable(`${what} removed`, async () => {
				await api.uploadProfileImage(kind, kept);
				version = Date.now();
				account = await api.account();
				await session.refresh();
			});
		} catch (e) {
			toasts.error(e);
		}
	}

	async function removeBackdrop() {
		const before = account?.banner_backdrop ?? '';
		await save({ banner_backdrop: '' });
		if (before) toasts.undoable('Banner removed', () => save({ banner_backdrop: before }));
	}

	async function chooseBackdrop(path: string) {
		if (account?.banner) await api.removeProfileImage('banner').catch(() => undefined);
		await save({ banner_backdrop: path }, 'Banner updated');
		choosingBanner = false;
	}

	function togglePin(titleId: number) {
		const pins = account?.pins ?? [];
		const on = pins.some((p) => p.title_id === titleId);
		if (!on && pins.length >= PINS_MAX) {
			toasts.push(`Pin up to ${PINS_MAX}; take one off first`, 'error');
			return;
		}
		void save({
			pins: on ? pins.filter((p) => p.title_id !== titleId) : [...pins, { title_id: titleId }]
		});
	}
</script>

<div class="profile card">
	<div class="banner">
		{#if account?.banner || account?.banner_backdrop}
			<img
				class="movable"
				src={account?.banner ? imageUrl('banner') : img(account?.banner_backdrop, 'w1280')}
				alt=""
				draggable="false"
				style:object-position="{bannerPos.x}% {bannerPos.y}%"
				onpointerdown={bannerDown}
				onpointermove={bannerMove}
				onpointerup={bannerUp}
				onpointercancel={bannerUp}
			/>
			<span class="hint" aria-hidden="true"><Icon name="move" size={14} /> Drag to move</span>
		{/if}
		<div class="banner-actions">
			<label class="btn small file">
				<Icon name="image" size={15} /> Upload banner
				<input
					type="file"
					accept="image/jpeg,image/png,image/webp,image/gif"
					disabled={busy}
					onchange={(e) => upload('banner', e.currentTarget.files?.[0])}
				/>
			</label>
			<button class="btn small" onclick={() => (choosingBanner = !choosingBanner)}>
				Use a backdrop
			</button>
			{#if account?.banner || account?.banner_backdrop}
				<button
					class="btn small"
					onclick={() => (account?.banner ? removeImage('banner') : removeBackdrop())}
				>
					Remove
				</button>
			{/if}
		</div>
	</div>

	{#if choosingBanner}
		<div class="picker">
			{#if backdrops.length}
				<p class="faint small">From the titles you rated highest.</p>
				<div class="backdrops">
					{#each backdrops as t (t.id)}
						<button
							class="backdrop"
							aria-label="Use {t.name}"
							onclick={() => chooseBackdrop(t.backdrop_path ?? '')}
						>
							<img src={img(t.backdrop_path, 'w300')} alt="" />
							<span>{t.name}</span>
						</button>
					{/each}
				</div>
			{:else}
				<p class="faint small">Add a few titles to your library to choose from their artwork.</p>
			{/if}
		</div>
	{/if}

	<div class="identity">
		<div class="picture">
			<Avatar src={account?.avatar ? imageUrl('avatar') : null} name={username || '?'} size={76} />
			<div class="picture-actions">
				<label class="btn ghost small file">
					<Icon name="camera" size={15} /> Picture
					<input
						type="file"
						accept="image/jpeg,image/png,image/webp,image/gif"
						disabled={busy}
						onchange={(e) => {
							void frame(e.currentTarget.files?.[0]);
							e.currentTarget.value = '';
						}}
					/>
				</label>
				{#if account?.avatar}
					<button class="btn ghost small" onclick={() => removeImage('avatar')}>Remove</button>
				{/if}
			</div>
		</div>

		<div class="fields">
			<label class="field">
				<span>Name</span>
				<input
					class="input"
					bind:value={name}
					maxlength="40"
					placeholder={username}
					onchange={() => save({ display_name: name }, 'Name saved')}
				/>
			</label>
			<label class="field">
				<span>About you <em class="faint">{bio.length}/{BIO_MAX}</em></span>
				<textarea
					class="input"
					rows="4"
					maxlength={BIO_MAX}
					bind:value={bio}
					placeholder="What you watch, what you love, what you are looking for."></textarea>
			</label>
			<div class="actions">
				<button
					class="btn"
					disabled={busy || bio === (account?.bio ?? '')}
					onclick={() => save({ bio }, 'About saved')}
				>
					Save about
				</button>
			</div>
		</div>
	</div>

	<div class="pins">
		<div class="pins-head">
			<span class="label">
				<strong>Pinned</strong>
				<span class="faint">Up to four titles at the top of your profile.</span>
			</span>
			<button class="btn ghost small" onclick={() => (choosingPins = !choosingPins)}>
				{choosingPins ? 'Done' : 'Choose'}
			</button>
		</div>
		{#if pinned.length}
			<div class="pinned">
				{#each pinned as t (t.id)}
					<span class="pin">
						{#if img(t.poster_path, 'w92')}<img src={img(t.poster_path, 'w92')} alt="" />{/if}
						<span>{t.name}</span>
					</span>
				{/each}
			</div>
		{/if}
		{#if choosingPins}
			<div class="choices">
				{#each titles.slice(0, 40) as t (t.id)}
					{@const on = (account?.pins ?? []).some((p) => p.title_id === t.id)}
					<button class="choice" class:on aria-pressed={on} onclick={() => togglePin(t.id)}>
						{t.name}
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<label class="row">
		<span class="label">
			<strong>Show my profile</strong>
			<span class="faint">
				Others on this Frog can see it. Off, only you can.
				{#if username}<a href="/people/{encodeURIComponent(username)}">See your profile</a>{/if}
			</span>
		</span>
		<input
			type="checkbox"
			class="toggle-switch"
			checked={account?.profile_public ?? false}
			onchange={(e) => save({ public: e.currentTarget.checked })}
		/>
	</label>
</div>

{#if framing}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" onclick={closeFrame}></div>
	<div class="framer card" role="dialog" aria-modal="true" aria-labelledby="framer-title">
		<h2 id="framer-title">Frame your picture</h2>
		<p class="faint small">Drag it into place, and zoom to fill the circle.</p>
		<div
			class="circle"
			style:width="{FRAME}px"
			style:height="{FRAME}px"
			onpointerdown={(e) => {
				(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
				pan = { x: e.clientX, y: e.clientY, from: { ...offset } };
			}}
			onpointermove={(e) => {
				if (pan) offset = within(pan.from.x + e.clientX - pan.x, pan.from.y + e.clientY - pan.y);
			}}
			onpointerup={() => (pan = null)}
			onpointercancel={() => (pan = null)}
			role="presentation"
		>
			<img
				src={framing.url}
				alt=""
				draggable="false"
				style:width="{drawn.w}px"
				style:height="{drawn.h}px"
				style:left="{(FRAME - drawn.w) / 2 + offset.x}px"
				style:top="{(FRAME - drawn.h) / 2 + offset.y}px"
			/>
		</div>
		<label class="zoom">
			<span>Zoom</span>
			<input
				type="range"
				min="1"
				max="3"
				step="0.01"
				bind:value={zoom}
				oninput={() => (offset = within(offset.x, offset.y))}
			/>
		</label>
		<div class="framer-actions">
			<button class="btn ghost" onclick={closeFrame}>Cancel</button>
			<button class="btn primary" disabled={busy} onclick={saveFrame}>Use this picture</button>
		</div>
	</div>
{/if}

<style>
	.profile {
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.banner {
		position: relative;
		height: 150px;
		background: linear-gradient(135deg, var(--accent-soft), var(--bg-elev-2));
	}

	.banner img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.banner .movable {
		cursor: grab;
		touch-action: none;
		user-select: none;
	}

	.banner .movable:active {
		cursor: grabbing;
	}

	/* Bottom right: the profile picture covers the bottom left, and the
	   buttons sit along the top. */
	.hint {
		position: absolute;
		right: 12px;
		bottom: 12px;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 10px;
		border-radius: 999px;
		font-size: 0.76rem;
		font-weight: 600;
		color: #fff;
		background: rgb(0 0 0 / 0.5);
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--fast) var(--ease);
	}

	.banner:hover .hint {
		opacity: 1;
	}

	/* No hover on a touchscreen, so it stays in view there. */
	@media (hover: none) {
		.hint {
			opacity: 1;
		}
	}

	.scrim {
		position: fixed;
		inset: 0;
		z-index: 95;
		background: rgb(0 0 0 / 0.55);
	}

	.framer {
		position: fixed;
		z-index: 96;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		width: min(340px, calc(100vw - 28px));
		padding: 22px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		box-shadow: var(--shadow-3);
	}

	.framer h2 {
		margin: 0;
		font-size: 1.15rem;
	}

	.framer p {
		margin: 0;
		text-align: center;
	}

	.circle {
		position: relative;
		overflow: hidden;
		border-radius: 50%;
		background: var(--bg-elev-2);
		box-shadow: 0 0 0 3px var(--accent);
		cursor: grab;
		touch-action: none;
		user-select: none;
	}

	.circle:active {
		cursor: grabbing;
	}

	.circle img {
		position: absolute;
		max-width: none;
		pointer-events: none;
	}

	.zoom {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		font-size: 0.82rem;
		color: var(--text-dim);
	}

	.zoom input {
		flex: 1;
		accent-color: var(--accent);
	}

	.framer-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		width: 100%;
	}

	/* Top corner: the picture is pulled up over the bottom of the banner,
	   and on a phone it reaches far enough across to cover the buttons. */
	.banner-actions {
		position: absolute;
		top: 12px;
		right: 12px;
		left: 12px;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		justify-content: flex-end;
	}

	.banner-actions .btn {
		background: rgb(0 0 0 / 0.55);
		color: #fff;
		border-color: transparent;
	}

	.file {
		position: relative;
		overflow: hidden;
	}

	.file input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.picker {
		padding: 14px 18px 0;
	}

	.backdrops {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 8px;
	}

	.backdrop {
		position: relative;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		overflow: hidden;
		padding: 0;
		cursor: pointer;
		aspect-ratio: 16 / 9;
		background: var(--bg-elev-2);
	}

	.backdrop img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.backdrop span {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		padding: 4px 6px;
		font-size: 0.72rem;
		text-align: left;
		color: #fff;
		background: linear-gradient(transparent, rgb(0 0 0 / 0.7));
	}

	.identity {
		display: flex;
		gap: 20px;
		padding: 18px;
		flex-wrap: wrap;
	}

	/* Pulled up over the banner, so it has to sit above it: the banner is
	   positioned (for its buttons), and positioned boxes paint on top. */
	.picture {
		position: relative;
		z-index: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		margin-top: -54px;
	}

	.picture :global(.avatar) {
		border: 4px solid var(--bg-elev);
	}

	.picture-actions {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.fields {
		flex: 1;
		min-width: 220px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.field em {
		font-style: normal;
		font-weight: 400;
	}

	.actions {
		display: flex;
		gap: 8px;
	}

	.pins {
		padding: 0 18px 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.pins-head,
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}

	.pinned {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
	}

	.pin {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 10px 4px 4px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		font-size: 0.85rem;
	}

	.pin img {
		width: 26px;
		height: 39px;
		border-radius: 4px;
		object-fit: cover;
	}

	.choices {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.choice {
		padding: 6px 10px;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--bg-elev);
		font-size: 0.82rem;
		cursor: pointer;
	}

	.choice.on {
		border-color: var(--accent);
		color: var(--accent);
	}

	.row {
		padding: 15px 18px;
		border-top: 1px solid var(--line);
		cursor: pointer;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.label .faint,
	.small {
		font-size: 0.8rem;
	}

	.label a {
		color: var(--accent);
	}

	input[type='checkbox']:not(.toggle-switch) {
		width: 20px;
		height: 20px;
		accent-color: var(--accent);
		flex: none;
	}
</style>
