<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import ImportRow from '#lib/components/ImportRow.svelte';
	import Select from '#lib/components/Select.svelte';
	import { CHOOSE, netflixRows, settleNetflix, type NetflixExport } from '#lib/import/netflix';
	import {
		dedupe,
		readFile,
		readNetflixFile,
		SOURCES,
		type Incoming,
		type ParsedFile
	} from '#lib/import/parsers';
	import {
		canKeepMatch,
		CLOSEST,
		groupRows,
		groupSummary,
		PROBLEM_KINDS,
		PROBLEM_LABEL,
		problemsOf,
		type ProblemKind,
		type ReviewRow as Row,
		type RowGroup
	} from '#lib/import/review';
	import { applyItem, bySeason, foldSeasons } from '#lib/import/rows';
	import { toasts } from '#lib/toast.svelte';
	import type { ImportSummary, SearchItem, Status } from '#lib/types';

	type Step = 'source' | 'profile' | 'review' | 'done';
	type Filter = 'all' | 'problems' | ProblemKind;

	let step = $state<Step>('source');
	let format = $state('mal');
	let rows = $state<Row[]>([]);
	let busy = $state(false);
	let progress = $state({ done: 0, total: 0, label: '' });
	let onConflict = $state<'skip' | 'overwrite'>('skip');
	let summary = $state<ImportSummary | null>(null);
	let filesRead = $state<string[]>([]);
	let notes = $state<string[]>([]);
	let filter = $state<Filter>('all');
	/** Shows opened or closed by hand; the rest follow `isOpen`. */
	let toggled = $state<Record<string, boolean>>({});
	/** A Netflix export with several profiles, waiting for one to be chosen. */
	let netflix = $state<{ data: NetflixExport; names: string[] } | null>(null);

	const source = $derived(SOURCES.find((s) => s.id === format) ?? SOURCES[0]);
	const included = $derived(rows.filter((r) => r.include && r.chosen));
	const tagged = $derived(rows.map((row) => ({ row, kinds: problemsOf(row) })));
	// A row left out is settled, but it stays on screen until the filter
	// changes, so a tick taken away by mistake is easy to put back.
	const flagged = $derived(tagged.filter((t) => t.kinds.length));
	const needLook = $derived(flagged.filter((t) => t.row.include).length);
	const chips = $derived(
		PROBLEM_KINDS.map((kind) => {
			const of = flagged.filter((t) => t.kinds.includes(kind));
			return { kind, any: of.length, count: of.filter((t) => t.row.include).length };
		}).filter((c) => c.any)
	);
	/** Falls back to everything once the last row of a kind is settled. */
	const active = $derived.by<Filter>(() => {
		if (filter === 'problems') return flagged.length ? 'problems' : 'all';
		return chips.some((c) => c.kind === filter) ? filter : 'all';
	});
	const shown = $derived.by(() => {
		if (active === 'all') return rows;
		if (active === 'problems') return flagged.map((t) => t.row);
		return flagged.filter((t) => t.kinds.includes(active)).map((t) => t.row);
	});
	const groups = $derived(groupRows(shown));
	// Only ever what is on screen: nothing changes that a person cannot see.
	const keepable = $derived(active === 'all' ? [] : shown.filter(canKeepMatch));
	const leavable = $derived(active === 'all' ? [] : shown.filter((r) => r.include && r.chosen));

	async function onFiles(list: FileList | null) {
		if (!list?.length) return;
		if (format === 'netflix') return readNetflixFiles(Array.from(list));
		await load(async () => {
			const chunks: ParsedFile[] = [];
			for (const file of Array.from(list)) chunks.push(...(await readFile(format, file)));
			return chunks;
		}, 'Reading the file');
	}

	async function readNetflixFiles(files: File[]) {
		busy = true;
		progress = { done: 0, total: 0, label: 'Reading the file' };
		try {
			const exports = await Promise.all(files.map(readNetflixFile));
			const data: NetflixExport = {
				plays: exports.flatMap((e) => e.plays),
				profiles: [...new Set(exports.flatMap((e) => e.profiles))].sort(),
				ignored: exports.reduce((n, e) => n + e.ignored, 0),
				undated: exports.reduce((n, e) => n + e.undated, 0),
				dateNote: exports.find((e) => e.dateNote)?.dateNote
			};
			const names = files.map((f) => f.name);
			if (data.profiles.length > 1) {
				netflix = { data, names };
				step = 'profile';
				return;
			}
			await useNetflix(data, null, names);
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function useNetflix(data: NetflixExport, profile: string | null, names: string[]) {
		netflix = null;
		const result = netflixRows(data, profile);
		await load(
			async () => names.map((name, i) => ({ name, rows: i === 0 ? result.rows : [] })),
			'Reading the file',
			result.notes
		);
	}

	async function load(read: () => Promise<ParsedFile[]>, label: string, extra: string[] = []) {
		busy = true;
		progress = { done: 0, total: 0, label };
		filesRead = [];
		notes = extra;

		try {
			const parsed: Incoming[] = [];
			for (const chunk of await read()) {
				filesRead.push(`${chunk.name} (${chunk.rows.length})`);
				parsed.push(...chunk.rows);
			}

			// Netflix rows arrive grouped already, one per show and season.
			const unique = format === 'netflix' ? parsed : dedupe(parsed);
			if (!unique.length) {
				toasts.push('Nothing to import in that file', 'error');
				return;
			}

			rows = unique.map((incoming) => ({ incoming, match: null, chosen: null, include: true }));
			step = 'review';
			await matchAll();
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	/** Matching happens in batches so progress is honest and TMDB is not flooded. */
	async function matchAll() {
		busy = true;
		progress = { done: 0, total: rows.length, label: 'Looking up titles' };

		const size = 40;
		for (let i = 0; i < rows.length; i += size) {
			const batch = rows.slice(i, i + size);
			try {
				const { results } = await api.matchImport(batch.map((r) => r.incoming));
				for (const result of results) {
					const row = rows.find((r) => r.incoming.key === result.key);
					if (!row) continue;
					row.match = result;
					row.chosen = result.matched;
				}
				rows = [...rows];
			} catch (e) {
				toasts.error(e);
				break;
			}
			progress = { ...progress, done: Math.min(i + size, rows.length) };
		}

		if (rows.some((r) => r.incoming.netflix)) rows = settleNetflix(rows);
		else foldAll();
		busy = false;
	}

	/** Later tracker seasons onto TMDB's long ones; see `foldSeasons`. */
	function foldAll() {
		foldSeasons(rows);
		rows = [...rows];
	}

	async function run() {
		busy = true;

		// "Keep what I have" is decided against the library as it was before
		// this import: otherwise season 2 of a show would be skipped because
		// season 1 was added a moment earlier in the same run.
		const keep = onConflict === 'skip';
		const skippedUp = keep ? included.filter((r) => r.match?.already_tracked).length : 0;
		const todo = included
			.filter((r) => !keep || !r.match?.already_tracked)
			// The latest season's status is the one the library ends up with.
			.sort(bySeason);

		summary = { added: 0, updated: 0, skipped: skippedUp, failed: [] };
		progress = { done: 0, total: todo.length, label: 'Adding to your library' };

		const size = 25;
		for (let i = 0; i < todo.length; i += size) {
			const batch = todo.slice(i, i + size);
			try {
				const result = await api.applyImport(batch.map(applyItem), 'overwrite');
				summary = {
					added: summary.added + result.added,
					updated: summary.updated + result.updated,
					skipped: summary.skipped + result.skipped,
					failed: [...summary.failed, ...result.failed]
				};
			} catch (e) {
				toasts.error(e);
				break;
			}
			progress = { ...progress, done: Math.min(i + size, todo.length) };
		}

		busy = false;
		step = 'done';
	}

	function choose(row: Row, candidate: SearchItem) {
		row.chosen = candidate;
		if (row.match) row.match.note = null;
		// Episodes found by name belong to the title they were found in.
		row.incoming.watches = undefined;
		rows = [...rows];
	}

	function setSeason(row: Row, value: string) {
		const n = Number(value);
		row.incoming.season = Number.isInteger(n) && n >= 0 ? n : null;
		// A season chosen by hand wins over the episodes found by name.
		row.incoming.watches = undefined;
		row.incoming.episode_offset = 0;
		const warning = row.incoming.warning ?? '';
		// Choosing the season settles any doubt about which one it was.
		const aboutSeason =
			warning === CHOOSE ||
			(row.incoming.netflix && /^Netflix (gives no season|calls this)/.test(warning));
		if (aboutSeason && row.incoming.season !== null) {
			row.incoming.warning = undefined;
		}
		rows = [...rows];
	}

	function setStatus(row: Row, status: Status) {
		row.incoming.status = status;
		rows = [...rows];
	}

	function setInclude(row: Row, include: boolean) {
		row.include = include;
		rows = [...rows];
	}

	function setShowIncluded(group: RowGroup<Row>, include: boolean) {
		for (const row of group.rows) row.include = include;
		rows = [...rows];
	}

	/** Everything is open with a question in it, or when looking at one kind of problem. */
	function isOpen(group: RowGroup<Row>): boolean {
		if (active !== 'all') return true;
		return toggled[group.key] ?? group.rows.some((r) => problemsOf(r).length);
	}

	function keepMatches() {
		const kept = [...keepable];
		for (const row of kept) row.match!.note = null;
		rows = [...rows];
		toasts.undoable(kept.length === 1 ? 'Match kept' : `${kept.length} matches kept`, () => {
			for (const row of kept) row.match!.note = CLOSEST;
			rows = [...rows];
		});
	}

	function leaveOut() {
		const left = [...leavable];
		for (const row of left) row.include = false;
		rows = [...rows];
		toasts.undoable(left.length === 1 ? 'Left out' : `${left.length} left out`, () => {
			for (const row of left) row.include = true;
			rows = [...rows];
		});
	}

	function restart() {
		rows = [];
		summary = null;
		filesRead = [];
		notes = [];
		netflix = null;
		filter = 'all';
		toggled = {};
		step = 'source';
	}

	const pct = $derived(progress.total ? Math.round((progress.done / progress.total) * 100) : 0);
</script>

<svelte:head>
	<title>Import · Frog</title>
</svelte:head>

<div class="page">
	<header class="head page-head">
		<div>
			<h1>Import</h1>
			<p class="sub">Bring a library across from another tracker, or restore a backup.</p>
		</div>
	</header>

	{#if step === 'source'}
		<div class="sources">
			{#each SOURCES as s (s.id)}
				<button
					class="source"
					class:on={format === s.id}
					aria-label={s.label}
					aria-pressed={format === s.id}
					onclick={() => (format = s.id)}
				>
					<strong>{s.label}</strong>
					<span class="faint">{s.hint}</span>
				</button>
			{/each}
		</div>

		<label class="drop">
			<input
				type="file"
				accept={source.accept}
				multiple
				disabled={busy}
				onchange={(e) => onFiles(e.currentTarget.files)}
			/>
			<Icon name="upload" size={26} />
			<strong>Choose a file</strong>
			<span class="faint">{source.label} · {source.accept.split(',').join(' or ')}</span>
		</label>
		<p class="faint note">Files are read in this browser; only titles are looked up on TMDB.</p>
		<p class="faint note">
			AniList stays in step on its own: connect it in
			<a href="/settings#connections">Settings</a>.
		</p>

		<div class="alt">
			<h2>Export</h2>
			<div class="actions">
				<a class="btn" href="/api/export/library.json" download>
					<Icon name="download" size={16} />
					Download backup
				</a>
				<a class="btn ghost" href="/api/export/library.csv" download>
					<Icon name="download" size={16} />
					Download CSV
				</a>
			</div>
		</div>
	{:else if step === 'profile' && netflix}
		<h2 class="pick-title">Whose viewing activity?</h2>
		<div class="sources">
			{#each netflix.data.profiles as name (name)}
				<button
					class="source"
					onclick={() => netflix && useNetflix(netflix.data, name, netflix.names)}
				>
					<strong>{name}</strong>
					<span class="faint">
						{netflix.data.plays.filter((p) => p.profile === name).length} plays
					</span>
				</button>
			{/each}
		</div>
		<div class="footer">
			<button class="btn ghost" onclick={restart}>Start over</button>
		</div>
	{:else if step === 'review'}
		{#if busy}
			<div class="progress card">
				<div class="bar"><i style:width="{pct}%"></i></div>
				<span>{progress.label} — {progress.done} of {progress.total}</span>
			</div>
		{/if}

		{#if filesRead.length}
			<p class="faint note">Read {filesRead.join(', ')}</p>
		{/if}
		{#each notes as line (line)}
			<p class="faint note">{line}</p>
		{/each}

		<div class="toolbar">
			<div class="counts">
				<strong>{included.length}</strong>
				<span class="faint">ready</span>
				{#if needLook}
					<strong class="warn">{needLook}</strong>
					<span class="faint">{needLook === 1 ? 'needs' : 'need'} a look</span>
				{/if}
			</div>

			<div class="toolbar-right">
				<Select
					value={onConflict}
					label="Already in your library"
					options={[
						{ value: 'skip', label: 'Keep what I have' },
						{ value: 'overwrite', label: 'Let the file win' }
					]}
					onchange={(v) => (onConflict = v as 'skip' | 'overwrite')}
				/>
			</div>
		</div>

		{#if flagged.length}
			<div class="chips" role="group" aria-label="Show">
				<button
					class="chip"
					class:on={active === 'all'}
					aria-pressed={active === 'all'}
					onclick={() => (filter = 'all')}
				>
					All <span class="n">{rows.length}</span>
				</button>
				<button
					class="chip"
					class:on={active === 'problems'}
					aria-pressed={active === 'problems'}
					onclick={() => (filter = 'problems')}
				>
					Needs a look <span class="n">{needLook}</span>
				</button>
				{#each chips as chip (chip.kind)}
					<button
						class="chip"
						class:on={active === chip.kind}
						aria-pressed={active === chip.kind}
						onclick={() => (filter = chip.kind)}
					>
						{PROBLEM_LABEL[chip.kind]} <span class="n">{chip.count}</span>
					</button>
				{/each}
			</div>
		{/if}

		{#if keepable.length || leavable.length}
			<div class="bulk">
				{#if keepable.length}
					<button class="btn ghost small" onclick={keepMatches}>
						<Icon name="check" size={14} />
						{keepable.length === 1 ? 'Keep this match' : `Keep these ${keepable.length} matches`}
					</button>
				{/if}
				{#if leavable.length}
					<button class="btn ghost small" onclick={leaveOut}>
						{leavable.length === 1 ? 'Leave this one out' : `Leave these ${leavable.length} out`}
					</button>
				{/if}
			</div>
		{/if}

		<ul class="rows">
			{#each groups as group (group.key)}
				<li>
					{#if group.rows.length === 1}
						<ImportRow
							row={group.rows[0]}
							oninclude={setInclude}
							onchoose={choose}
							onseason={setSeason}
							onstatus={setStatus}
						/>
					{:else}
						{@const first = group.rows[0].chosen}
						{@const on = group.rows.filter((r) => r.include).length}
						{@const open = isOpen(group)}
						<div class="show">
							<div class="show-head">
								<input
									type="checkbox"
									checked={on === group.rows.length}
									indeterminate={on > 0 && on < group.rows.length}
									aria-label="Include every row of {first?.name}"
									onchange={(e) => setShowIncluded(group, e.currentTarget.checked)}
								/>
								{#if first?.poster_path}
									<img src={img(first.poster_path, 'w92')} alt="" loading="lazy" />
								{:else}
									<span class="no-art"><Icon name="tv" size={15} /></span>
								{/if}
								<div class="show-name">
									<div class="line">
										<strong>{first?.name}</strong>
										{#if first?.year}<span class="faint">{first.year}</span>{/if}
									</div>
									<span class="faint summary">{groupSummary(group.rows)}</span>
								</div>
								{#if active === 'all'}
									<button
										class="show-toggle"
										aria-expanded={open}
										aria-label="{open ? 'Hide' : 'Show'} the rows of {first?.name}"
										onclick={() => (toggled[group.key] = !open)}
									>
										<Icon name={open ? 'chevron-up' : 'chevron-down'} size={16} />
									</button>
								{/if}
							</div>
							{#if open}
								{#each group.rows as row (row.incoming.key)}
									<ImportRow
										{row}
										grouped
										oninclude={setInclude}
										onchoose={choose}
										onseason={setSeason}
										onstatus={setStatus}
									/>
								{/each}
							{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ul>

		<div class="footer">
			<button class="btn ghost" onclick={restart} disabled={busy}>Start over</button>
			<button class="btn primary" onclick={run} disabled={busy || !included.length}>
				Import {included.length}
				{included.length === 1 ? 'title' : 'titles'}
			</button>
		</div>
	{:else if summary}
		<div class="done card">
			<Icon name="check" size={30} />
			<h2>Import finished</h2>
			<dl>
				<div>
					<dt>Added</dt>
					<dd>{summary.added}</dd>
				</div>
				<div>
					<dt>Updated</dt>
					<dd>{summary.updated}</dd>
				</div>
				<div>
					<dt>Skipped</dt>
					<dd>{summary.skipped}</dd>
				</div>
				{#if summary.failed.length}
					<div>
						<dt>Failed</dt>
						<dd class="bad">{summary.failed.length}</dd>
					</div>
				{/if}
			</dl>
			<div class="actions">
				<button class="btn primary" onclick={() => goto('/library')}>Open library</button>
				<button class="btn ghost" onclick={restart}>Import something else</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.head {
		margin-bottom: 26px;
	}

	.head p {
		margin-top: 6px;
		font-size: 0.95rem;
	}

	.sources {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
		gap: 12px;
		margin-bottom: 20px;
	}

	.source {
		display: flex;
		flex-direction: column;
		gap: 5px;
		padding: 15px 17px;
		text-align: left;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		cursor: pointer;
		transition: border-color var(--fast) var(--ease);
	}

	.source.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.source .faint {
		font-size: 0.78rem;
		line-height: 1.45;
	}

	.drop {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 46px 20px;
		border: 1px dashed var(--line-strong);
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--bg-elev) 55%, transparent);
		cursor: pointer;
		transition: border-color var(--fast) var(--ease);
	}

	.pick-title {
		margin-bottom: 14px;
	}

	.drop:hover {
		border-color: var(--accent);
	}

	.drop > input[type='file'] {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.note {
		margin-top: 14px;
		font-size: 0.8rem;
		max-width: 62ch;
	}

	.alt {
		margin-top: 44px;
		padding-top: 24px;
		border-top: 1px solid var(--line);
	}

	.alt h2 {
		margin-bottom: 12px;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
	}

	.progress {
		padding: 16px 18px;
		margin-bottom: 18px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		font-size: 0.86rem;
	}

	.bar {
		height: 5px;
		border-radius: var(--radius-pill);
		background: var(--line-strong);
		overflow: hidden;
	}

	.bar i {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width var(--slow) var(--ease);
	}

	.toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
		margin-bottom: 14px;
	}

	.counts {
		display: flex;
		align-items: baseline;
		gap: 7px;
		font-size: 0.88rem;
	}

	.counts .warn {
		color: var(--warn);
		margin-left: 10px;
	}

	.toolbar-right {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.btn.small {
		padding: 7px 13px;
		font-size: 0.8rem;
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.bulk {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 10px;
	}

	.chips + .rows,
	.bulk + .rows {
		margin-top: 14px;
	}

	.chip .n {
		font-variant-numeric: tabular-nums;
		opacity: 0.7;
	}

	.show {
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		overflow: hidden;
	}

	.show-head {
		display: grid;
		grid-template-columns: auto 40px minmax(0, 1fr) auto;
		align-items: center;
		gap: 14px;
		padding: 10px 14px;
	}

	.show-head img,
	.no-art {
		width: 40px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		display: grid;
		place-items: center;
		background: var(--bg-elev-2);
		color: var(--text-faint);
		object-fit: cover;
	}

	.show-head input[type='checkbox'] {
		width: 18px;
		height: 18px;
		accent-color: var(--accent);
	}

	.show-name {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.line {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}

	.line strong {
		font-size: 0.92rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.summary {
		font-size: 0.75rem;
	}

	.show-toggle {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text-faint);
		cursor: pointer;
	}

	.show-toggle:hover {
		color: var(--text);
		background: var(--bg-elev-2);
	}

	.footer {
		display: flex;
		justify-content: flex-end;
		gap: 10px;
		margin-top: 24px;
		position: sticky;
		bottom: 0;
		padding: 14px 0;
		background: linear-gradient(transparent, var(--bg) 35%);
	}

	.done {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 14px;
		padding: 46px 24px;
		text-align: center;
		color: var(--good);
	}

	.done h2 {
		color: var(--text);
	}

	.done dl {
		display: flex;
		gap: 30px;
		margin: 6px 0 10px;
	}

	.done div {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	dt {
		font-size: 0.74rem;
		color: var(--text-faint);
		text-transform: uppercase;
		letter-spacing: 0.07em;
	}

	dd {
		margin: 0;
		font-size: 1.5rem;
		font-weight: 650;
		color: var(--text);
		font-family: var(--font-display);
	}

	dd.bad {
		color: var(--bad);
	}
</style>
