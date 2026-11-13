<script lang="ts">
	import { backCloses } from '#lib/layers.svelte';
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import BadgeSettings from '#lib/components/settings/BadgeSettings.svelte';
	import ProfileLook from '#lib/components/settings/ProfileLook.svelte';
	import { AVATAR_PIXELS, BANNER_PIXELS, decode, encode, shrink, staysAsItIs } from '#lib/picture';
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
	backCloses(
		() => choosingBanner,
		() => (choosingBanner = false)
	);

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
	let framing = $state<{ url: string; type: string; width: number; height: number } | null>(null);
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
		if (await staysAsItIs(file)) return upload('avatar', file);
		try {
			const bitmap = await decode(file);
			framing = {
				url: URL.createObjectURL(file),
				type: file.type,
				width: bitmap.width,
				height: bitmap.height
			};
			bitmap.close();
			zoom = 1;
			offset = { x: 0, y: 0 };
		} catch (e) {
			toasts.error(e);
		}
	}

	function closeFrame() {
		if (framing) URL.revokeObjectURL(framing.url);
		framing = null;
	}
	backCloses(() => !!framing, closeFrame);

	async function saveFrame() {
		if (!framing) return;
		const image = new Image();
		image.src = framing.url;
		await image.decode();
		const size = AVATAR_PIXELS;
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
		const source = framing.type;
		closeFrame();
		await send('avatar', () => encode(canvas, source));
	}
	let choosingPins = $state(false);
	backCloses(
		() => choosingPins,
		() => (choosingPins = false)
	);

	// How tall the banner is across the top, which shapes the frame it is dragged in.
	const layout = $derived(account?.banner_layout ?? 'classic');
	const height = $derived(account?.banner_height ?? 'short');

	// The session first: it is what a rename on this same page updates.
	const username = $derived(session.status?.username ?? account?.username ?? '');
	const imageUrl = (kind: 'avatar' | 'banner') =>
		`/api/profiles/${encodeURIComponent(username)}/images/${kind}?v=${version}`;
	const bannerSrc = $derived(
		account?.banner
			? imageUrl('banner')
			: account?.banner_backdrop
				? img(account.banner_backdrop, 'w1280')
				: null
	);
	const pinned = $derived(account?.pinned ?? []);

	// Finding a title to pin: the best rated to start, then the whole library
	// by name as you type.
	let pinTerm = $state('');
	let pinFound = $state<LibraryItem[]>([]);
	$effect(() => {
		const q = pinTerm.trim();
		if (!q) {
			pinFound = [];
			return;
		}
		let current = true;
		const timer = setTimeout(() => {
			void api
				.library({ q, sort: 'name', order: 'asc', limit: 40 })
				.then((r) => {
					if (current) pinFound = r.items;
				})
				.catch(() => {
					if (current) pinFound = [];
				});
		}, 200);
		return () => {
			current = false;
			clearTimeout(timer);
		};
	});
	const pinChoices = $derived(pinTerm.trim() ? pinFound : titles.slice(0, 24));
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

	/** Pictures are made ready here, so the server never has to decode one. */
	async function send(kind: 'avatar' | 'banner', ready: () => Promise<Blob>) {
		busy = true;
		try {
			await api.uploadProfileImage(kind, await ready());
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

	async function upload(kind: 'avatar' | 'banner', file: File | undefined) {
		if (file)
			await send(kind, () => shrink(file, kind === 'avatar' ? AVATAR_PIXELS : BANNER_PIXELS));
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
	<!-- As tall as the profile shows it across the top. A vertical banner's
	     column has its own frame under Banner, below. -->
	<div
		class="banner"
		class:tall={layout !== 'vertical' && height === 'tall'}
		class:full={layout !== 'vertical' && height === 'full'}
	>
		<span class="frame">
			{#if account?.banner || account?.banner_backdrop}
				<img
					class="movable"
					src={bannerSrc}
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
		</span>
		<div class="banner-actions">
			<label class="btn small file">
				<Icon name="image" size={15} /> Upload banner
				<input
					type="file"
					accept="image/jpeg,image/png,image/webp,image/gif"
					disabled={busy}
					onchange={(e) => {
						void upload('banner', e.currentTarget.files?.[0]);
						e.currentTarget.value = '';
					}}
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
		<!-- Its own layer, over the page: inline it slid under the picture,
		     which is pulled up over the banner. -->
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="picker-scrim" onclick={() => (choosingBanner = false)}></div>
		<div class="picker card" role="dialog" aria-modal="true" aria-labelledby="picker-title">
			<div class="picker-head">
				<h3 id="picker-title">Choose a backdrop</h3>
				<button class="btn ghost small" aria-label="Close" onclick={() => (choosingBanner = false)}>
					<Icon name="x" size={16} />
				</button>
			</div>
			{#if backdrops.length}
				<p class="faint small">From the titles you rated highest.</p>
				<div class="backdrops">
					{#each backdrops as t (t.id)}
						<button
							class="backdrop"
							aria-label="Use {t.name}"
							onclick={() => chooseBackdrop(t.backdrop_path ?? '')}
						>
							<img src={img(t.backdrop_path, 'w300')} alt="" loading="lazy" />
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
				<strong>Pinned <em class="faint">{pinned.length}/{PINS_MAX}</em></strong>
				<span class="faint">Up to four titles at the top of your profile.</span>
			</span>
			<button
				class="btn ghost small"
				onclick={() => {
					choosingPins = !choosingPins;
					pinTerm = '';
				}}
			>
				{choosingPins ? 'Done' : 'Add'}
			</button>
		</div>
		{#if pinned.length}
			<ul class="pinned">
				{#each pinned as t (t.id)}
					<li class="pin">
						{#if img(t.poster_path, 'w92')}<img src={img(t.poster_path, 'w92')} alt="" />{/if}
						<span class="pin-name">{t.name}</span>
						<button class="unpin hit" aria-label="Unpin {t.name}" onclick={() => togglePin(t.id)}>
							<Icon name="x" size={14} />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
		{#if choosingPins}
			<div class="finder">
				<input
					class="input"
					type="search"
					bind:value={pinTerm}
					placeholder="Find a title in your library"
					aria-label="Find a title to pin"
					autocomplete="off"
					spellcheck="false"
				/>
				<div class="choices">
					{#each pinChoices as t (t.id)}
						{@const on = (account?.pins ?? []).some((p) => p.title_id === t.id)}
						<button class="choice" class:on aria-pressed={on} onclick={() => togglePin(t.id)}>
							{#if on}<Icon name="check" size={13} />{/if}
							{t.name}
						</button>
					{:else}
						<p class="faint small">
							{pinTerm.trim()
								? 'Nothing in your library by that name.'
								: 'Add titles to your library to pin them.'}
						</p>
					{/each}
				</div>
			</div>
		{/if}
	</div>

	<ProfileLook {account} {save} banner={bannerSrc} />

	<BadgeSettings />

	<label class="row">
		<span class="label">
			<strong>Show my profile</strong>
			<span class="faint">
				Others on this Frog can see your profile, your whole library, your ratings and what you
				watched lately on their Home, but not your notes. Off, only you can.
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

	.banner .frame {
		position: absolute;
		inset: 0;
		display: block;
		overflow: hidden;
	}

	.banner.tall {
		height: 230px;
	}

	/* Full: the picture whole, up to a ceiling, as on the profile. */
	.banner.full {
		height: auto;
		min-height: 150px;
	}

	.banner.full .frame {
		position: relative;
	}

	.banner.full img {
		height: auto;
		max-height: 320px;
		display: block;
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

	.picker-scrim {
		position: fixed;
		inset: 0;
		z-index: 199;
		background: rgb(0 0 0 / 0.5);
		animation: fade 160ms ease backwards;
	}

	.picker {
		position: fixed;
		z-index: 200;
		top: 50%;
		left: 50%;
		width: min(720px, calc(100vw - 32px));
		max-height: min(80dvh, 640px);
		overflow-y: auto;
		padding: 18px;
		transform: translate(-50%, -50%);
		box-shadow: var(--shadow-3);
		animation: pop 220ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}

	.picker-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 4px;
	}

	.picker-head h3 {
		margin: 0;
		font-size: 1.05rem;
	}

	.picker .small {
		margin-bottom: 12px;
	}

	@keyframes fade {
		from {
			opacity: 0;
		}
	}

	@keyframes pop {
		from {
			opacity: 0;
			transform: translate(-50%, -46%) scale(0.98);
		}
	}

	/* A phone: a sheet from the bottom, in reach of a thumb. */
	@media (max-width: 600px) {
		.picker {
			top: auto;
			left: 10px;
			right: 10px;
			bottom: calc(10px + env(safe-area-inset-bottom));
			width: auto;
			transform: none;
			border-radius: 18px;
			animation-name: sheet;
		}

		.backdrops {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}

	@keyframes sheet {
		from {
			opacity: 0;
			transform: translateY(24px);
		}
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
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.pin {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 4px 4px 4px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		font-size: 0.85rem;
		min-width: 0;
		max-width: 100%;
	}

	.pin-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.unpin {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 50%;
		background: none;
		color: var(--text-dim);
		cursor: pointer;
	}

	.unpin:hover {
		background: var(--bg-hover);
		color: var(--text);
	}

	.finder {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.pins-head em {
		font-style: normal;
		font-weight: 400;
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

	.choice {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
