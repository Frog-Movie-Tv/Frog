<script lang="ts">
	import { addFrom, chooseStatus } from '#lib/add';
	import { api, img } from '#lib/api';
	import Avatar from '#lib/components/Avatar.svelte';
	import Empty from '#lib/components/Empty.svelte';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import LoadFailed from '#lib/components/LoadFailed.svelte';
	import LoadMore from '#lib/components/LoadMore.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { failure, type Failure } from '#lib/failure';
	import { relativeDate } from '#lib/format';
	import { inbox } from '#lib/inbox.svelte';
	import { avatarOf, personName } from '#lib/people.svelte';
	import { toasts } from '#lib/toast.svelte';
	import { statusLabel, type Notice, type Recommendation, type Status } from '#lib/types';

	/*
	 * Everything Frog told this account, newest first, and the titles people
	 * recommended to it. For you only appears once somebody has recommended
	 * something; until then the page is Everything, with no tabs.
	 */

	let items = $state<Notice[]>([]);
	let forYou = $state<Recommendation[]>([]);
	let next = $state<number | null>(null);
	let loading = $state(true);
	let more = $state(false);
	let failed = $state<Failure | null>(null);
	let tab = $state<'for-you' | 'all'>('all');
	let opened = false;

	async function load() {
		loading = true;
		failed = null;
		try {
			const r = await api.notifications();
			items = r.items;
			forYou = r.for_you ?? [];
			next = r.next;
			// Something waiting for an answer: that is what the page opens on.
			// Read again after a change, it stays where it is.
			if (!opened) tab = forYou.some((x) => !x.answered && !x.title.status) ? 'for-you' : 'all';
			opened = true;
			inbox.set(r.unread);
			// Seeing them is reading them.
			const unseen = items.filter((i) => !i.read).map((i) => i.id);
			if (unseen.length) inbox.set((await api.markRead(unseen)).unread);
		} catch (e) {
			failed = failure(e, {
				title: 'Could not load your notifications',
				hint: 'Try again in a minute.'
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	async function loadMore() {
		if (!next || more) return;
		more = true;
		try {
			const r = await api.notifications(next);
			const seen = new Set(items.map((i) => i.id));
			items = [...items, ...r.items.filter((i) => !seen.has(i.id))];
			next = r.next;
			const unseen = r.items.filter((i) => !i.read).map((i) => i.id);
			if (unseen.length) inbox.set((await api.markRead(unseen)).unread);
		} catch (e) {
			toasts.error(e);
		} finally {
			more = false;
		}
	}

	async function markAll() {
		try {
			inbox.set((await api.markRead()).unread);
			items = items.map((i) => ({ ...i, read: true }));
		} catch (e) {
			toasts.error(e);
		}
	}

	/** Into your library with the status you choose, and the recommendation answered. */
	async function add(rec: Recommendation, e: MouseEvent) {
		const done = await addFrom(rec.title, e.currentTarget as HTMLElement, e, load);
		if (!done) return;
		try {
			await api.answerRecommendation(rec.id, 'added');
			forYou = forYou.map((r) =>
				r.id === rec.id
					? { ...r, answered: 'added', title: { ...r.title, status: done.status } }
					: r
			);
		} catch (err) {
			toasts.error(err);
		}
	}

	async function dismiss(rec: Recommendation) {
		try {
			await api.answerRecommendation(rec.id, 'dismissed');
			forYou = forYou.filter((r) => r.id !== rec.id);
			toasts.undoable(`Dismissed ${rec.title.name}`, async () => {
				await api.answerRecommendation(rec.id, rec.answered);
				await load();
			});
		} catch (e) {
			toasts.error(e);
		}
	}

	/** Someone says you watched it with them: into your own diary, or not you at all. */
	async function together(n: Notice, action: 'add' | 'not_me', e?: MouseEvent) {
		try {
			let status: Status | undefined;
			if (action === 'add' && n.title && !n.title.status && e) {
				const chosen = await chooseStatus(n.title, e.currentTarget as HTMLElement, { event: e });
				if (!chosen) return;
				status = chosen;
			}
			const r = await api.answerTogether(n.id, action, status);
			if (action === 'not_me') {
				items = items.filter((i) => i.id !== n.id);
				toasts.push('Taken off');
			} else {
				items = items.map((i) =>
					i.id === n.id ? { ...i, payload: { ...(i.payload ?? {}), answered: 'added' } } : i
				);
				toasts.push(r.copied ? `Added to your diary` : 'Already in your diary');
			}
		} catch (err) {
			toasts.error(err);
		}
	}

	/** Today, This week, Earlier: by the day here. */
	function bucket(iso: string): 'Today' | 'This week' | 'Earlier' {
		const then = new Date(iso);
		const now = new Date();
		const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
		if (then.getTime() >= start) return 'Today';
		if (then.getTime() >= start - 6 * 86_400_000) return 'This week';
		return 'Earlier';
	}

	const groups = $derived.by(() => {
		const out: { name: string; items: Notice[] }[] = [];
		for (const item of items) {
			const name = bucket(item.created_at);
			const last = out.at(-1);
			if (last?.name === name) last.items.push(item);
			else out.push({ name, items: [item] });
		}
		return out;
	});

	const unread = $derived(items.some((i) => !i.read) || inbox.unread > 0);

	/** What one line says, and where it leads. */
	function line(n: Notice): { text: string; detail?: string; href: string | null; icon: IconName } {
		const who = n.actor ? personName(n.actor) : 'Someone';
		const name = n.title?.name ?? 'A title';
		const titleHref = n.title ? `/t/${n.title.id}` : null;
		const p = (n.payload ?? {}) as Record<string, unknown>;
		const text = (k: string) => (typeof p[k] === 'string' ? (p[k] as string) : undefined);
		switch (n.kind) {
			case 'recommendation':
				return {
					text: `${who} recommended ${name}`,
					detail: text('note'),
					href: titleHref,
					icon: 'share'
				};
			case 'added':
				return { text: `${who} added ${name}`, href: titleHref, icon: 'check' };
			case 'episode':
				return { text: name, detail: text('body'), href: titleHref, icon: 'tv' };
			case 'film':
				return { text: `${name} is out today`, href: titleHref, icon: 'film' };
			case 'badge':
				return {
					text: 'You were given a badge',
					detail: text('name'),
					href: text('username') ? `/people/${encodeURIComponent(text('username')!)}` : null,
					icon: 'crown'
				};
			case 'update':
				return {
					text: `Frog is now ${text('version') ?? 'up to date'}`,
					detail: "See what's new",
					href: '/whats-new',
					icon: 'sparkle'
				};
			case 'ops':
				return {
					text: text('title') ?? 'Something on the server failed',
					detail: text('body'),
					href: '/admin#server',
					icon: 'server'
				};
			case 'together':
				return {
					text: text('text') ?? `${who} watched ${name} with you`,
					href: titleHref,
					icon: 'users'
				};
		}
		return { text: name, href: titleHref, icon: 'bell' };
	}
</script>

<svelte:head>
	<title>Notifications · Frog</title>
</svelte:head>

<div class="page">
	<div class="page-head">
		<h1>Notifications</h1>
		{#if !loading && unread}
			<div class="actions">
				<button class="btn ghost small" onclick={markAll}>Mark all read</button>
			</div>
		{/if}
	</div>

	{#if loading && !items.length && !forYou.length}
		<Spinner label="Loading notifications" />
	{:else if failed}
		<LoadFailed {failed} onretry={load} />
	{:else}
		{#if forYou.length}
			<div class="segmented" role="group" aria-label="Show">
				<button class:on={tab === 'for-you'} onclick={() => (tab = 'for-you')}>For you</button>
				<button class:on={tab === 'all'} onclick={() => (tab = 'all')}>Everything</button>
			</div>
		{/if}

		{#if tab === 'for-you' && forYou.length}
			<ul class="recs">
				{#each forYou as rec (rec.id)}
					<li class="rec card">
						<a class="poster" href="/t/{rec.title.id}" aria-label={rec.title.name}>
							{#if rec.title.poster_path}
								<img src={img(rec.title.poster_path, 'w185')} alt="" loading="lazy" />
							{/if}
						</a>
						<div class="body">
							<a class="name" href="/t/{rec.title.id}">
								{rec.title.name}{#if rec.title.year}<span class="faint">
										· {rec.title.year}</span
									>{/if}
							</a>
							<span class="from faint">
								<Avatar src={avatarOf(rec.from)} name={personName(rec.from)} size={18} />
								{personName(rec.from)} · {relativeDate(rec.created_at)}
							</span>
							{#if rec.note}<p class="note">“{rec.note}”</p>{/if}
							<div class="answer">
								{#if rec.title.status}
									<span class="pill" data-status={rec.title.status}>
										{statusLabel(rec.title.status, rec.title.kind)}
									</span>
								{:else}
									<button
										class="btn primary small"
										aria-haspopup="menu"
										onclick={(e) => add(rec, e)}
									>
										<Icon name="plus" size={14} /> Add
									</button>
								{/if}
								<button class="btn ghost small" onclick={() => dismiss(rec)}>Dismiss</button>
							</div>
						</div>
					</li>
				{/each}
			</ul>
		{:else if !items.length}
			<Empty
				art="rest"
				title="Nothing yet"
				hint="New episodes, recommendations and badges show up here. Recommend a title from its page."
			>
				<a class="btn primary" href="/library">Open library</a>
			</Empty>
		{:else}
			{#each groups as group (group.name)}
				<section class="group">
					<h2>{group.name}</h2>
					<ul class="list-pane notices">
						{#each group.items as n (n.id)}
							{@const said = line(n)}
							<li class:unread={!n.read}>
								<svelte:element this={said.href ? 'a' : 'div'} class="notice" href={said.href}>
									<span class="art">
										{#if n.title?.poster_path}
											<img src={img(n.title.poster_path, 'w92')} alt="" loading="lazy" />
										{:else if n.actor}
											<Avatar src={avatarOf(n.actor)} name={personName(n.actor)} size={36} />
										{:else}
											<Icon name={said.icon} size={18} />
										{/if}
									</span>
									<span class="words">
										<strong>{said.text}</strong>
										{#if said.detail}<span class="detail">{said.detail}</span>{/if}
									</span>
									<time class="faint" datetime={n.created_at}>{relativeDate(n.created_at)}</time>
								</svelte:element>
								{#if n.kind === 'together' && n.payload?.answered !== 'added'}
									<div class="together">
										<button
											class="btn small"
											aria-haspopup={n.title?.status ? undefined : 'menu'}
											onclick={(e) => together(n, 'add', e)}
										>
											Add to my diary
										</button>
										<button class="btn ghost small" onclick={() => together(n, 'not_me')}>
											Not me
										</button>
									</div>
								{/if}
							</li>
						{/each}
					</ul>
				</section>
			{/each}
			<LoadMore more={!!next} loading={more} onmore={loadMore} />
		{/if}
	{/if}
</div>

<style>
	.page {
		max-width: 760px;
	}

	.segmented {
		margin-bottom: 18px;
	}

	.group {
		margin-bottom: 24px;
	}

	h2 {
		margin-bottom: 10px;
		font-size: 0.95rem;
		color: var(--text-dim);
	}

	.notices {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.notice {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		color: inherit;
		text-decoration: none;
	}

	a.notice:hover {
		background: var(--bg-hover);
	}

	.notices li + li {
		border-top: 1px solid var(--line);
	}

	.art {
		flex: none;
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		color: var(--text-dim);
		overflow: hidden;
	}

	.art img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.words {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.words strong {
		font-size: 0.9rem;
		font-weight: 600;
	}

	.unread .words strong::before {
		content: '';
		display: inline-block;
		width: 7px;
		height: 7px;
		margin-right: 7px;
		border-radius: 50%;
		background: var(--accent);
		vertical-align: middle;
	}

	.detail {
		font-size: 0.8rem;
		color: var(--text-dim);
	}

	time {
		flex: none;
		font-size: 0.74rem;
	}

	.recs {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 12px;
	}

	.rec {
		display: flex;
		gap: 14px;
		padding: 12px;
	}

	.poster {
		flex: none;
		width: 72px;
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.poster img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		flex: 1;
	}

	.name {
		font-weight: 650;
		color: inherit;
		text-decoration: none;
	}

	.from {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 0.8rem;
	}

	.note {
		margin: 2px 0;
		font-size: 0.86rem;
		color: var(--text-dim);
	}

	.answer {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: auto;
		padding-top: 6px;
	}

	/* A watch together's answer, under its line. */
	.together {
		display: flex;
		gap: 8px;
		padding: 0 14px 12px 62px;
	}
</style>
