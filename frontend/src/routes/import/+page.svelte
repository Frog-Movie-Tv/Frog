<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, img } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
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
	import { applyItem, bySeason, foldSeasons } from '#lib/import/rows';
	import { toasts } from '#lib/toast.svelte';
	import type { ImportSummary, MatchResult, SearchItem, Status } from '#lib/types';
	import { STATUS_LABEL, STATUSES } from '#lib/types';

	type Step = 'source' | 'profile' | 'review' | 'done';

	interface Row {
		incoming: Incoming;
		match: MatchResult | null;
		chosen: SearchItem | null;
		include: boolean;
	}

	let step = $state<Step>('source');
	let format = $state('mal');
	let rows = $state<Row[]>([]);
	let busy = $state(false);
	let progress = $state({ done: 0, total: 0, label: '' });
	let onConflict = $state<'skip' | 'overwrite'>('skip');
	let summary = $state<ImportSummary | null>(null);
	let filesRead = $state<string[]>([]);
	let notes = $state<string[]>([]);
	let onlyProblems = $state(false);
	/** A Netflix export with several profiles, waiting for one to be chosen. */
	let netflix = $state<{ data: NetflixExport; names: string[] } | null>(null);

	const source = $derived(SOURCES.find((s) => s.id === format) ?? SOURCES[0]);
	const included = $derived(rows.filter((r) => r.include && r.chosen));
	const problems = $derived(rows.filter((r) => !r.chosen || r.match?.note || r.incoming.warning));
	const shown = $derived(onlyProblems ? problems : rows);

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

	function restart() {
		rows = [];
		summary = null;
		filesRead = [];
		notes = [];
		netflix = null;
		step = 'source';
	}

	const pct = $derived(progress.total ? Math.round((progress.done / progress.total) * 100) : 0);
</script>

<svelte:head>
	<title>Import · Frog</title>
</svelte:head>

<div class="page">
	<header class="head">
		<h1>Import</h1>
		<p class="muted">Bring a library across from another tracker, or restore a backup.</p>
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
				{#if problems.length}
					<strong class="warn">{problems.length}</strong>
					<span class="faint">need a look</span>
				{/if}
			</div>

			<div class="toolbar-right">
				<button
					class="btn ghost small"
					class:on={onlyProblems}
					onclick={() => (onlyProblems = !onlyProblems)}
				>
					{onlyProblems ? 'Show all' : 'Show only problems'}
				</button>
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

		<ul class="rows">
			{#each shown as row (row.incoming.key)}
				<li class="row" class:off={!row.include} class:unmatched={!row.chosen}>
					<input
						type="checkbox"
						checked={row.include}
						disabled={!row.chosen}
						aria-label="Include {row.incoming.title}"
						onchange={(e) => {
							row.include = e.currentTarget.checked;
							rows = [...rows];
						}}
					/>

					{#if row.chosen?.poster_path}
						<img src={img(row.chosen.poster_path, 'w92')} alt="" loading="lazy" />
					{:else}
						<span class="no-art"><Icon name="film" size={15} /></span>
					{/if}

					<div class="what">
						<div class="line">
							<strong>{row.chosen?.name ?? row.incoming.title}</strong>
							{#if row.chosen?.year}<span class="faint">{row.chosen.year}</span>{/if}
							{#if row.chosen}
								<span class="pill tiny">{row.chosen.kind === 'tv' ? 'Show' : 'Film'}</span>
							{/if}
						</div>
						<div class="sub faint">
							<span
								>{row.incoming.label ?? row.incoming.title}{row.incoming.year
									? ` (${row.incoming.year})`
									: ''}</span
							>
							{#if row.incoming.progress}<span>{row.incoming.progress} watched</span>{/if}
							{#if row.incoming.rating !== null}<span>rated {Math.round(row.incoming.rating)}</span
								>{/if}
							{#if row.match?.already_tracked}
								<span class="already">already in library</span>
							{/if}
						</div>

						{#if row.incoming.warning}
							<p class="note-line">{row.incoming.warning}</p>
						{/if}
						{#if row.incoming.info}
							<p class="info-line">{row.incoming.info}</p>
						{/if}
						{#if row.match?.note}
							<p class="note-line">{row.match.note}</p>
						{/if}

						{#if row.match?.candidates?.length}
							<div class="candidates">
								{#each row.match.candidates as c (`${c.kind}-${c.tmdb_id}`)}
									<button class="cand" onclick={() => choose(row, c)}>
										{c.name}{c.year ? ` (${c.year})` : ''}
									</button>
								{/each}
							</div>
						{/if}
					</div>

					<div class="row-controls">
						{#if row.chosen?.kind === 'tv' && (row.incoming.anime || row.incoming.netflix)}
							<label class="season" title="TMDB season">
								<span>S</span>
								<input
									type="number"
									min="0"
									inputmode="numeric"
									aria-label="Season for {row.incoming.label ?? row.incoming.title}"
									value={row.incoming.season ?? ''}
									placeholder="?"
									onchange={(e) => setSeason(row, e.currentTarget.value)}
								/>
							</label>
						{/if}
						<Select
							value={row.incoming.status ?? 'completed'}
							label="Status"
							compact
							options={STATUSES.map((s) => ({ value: s, label: STATUS_LABEL[s] }))}
							onchange={(v) => setStatus(row, v as Status)}
						/>
					</div>
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

	.row-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.season {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px 0 9px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		font-size: 0.78rem;
		color: var(--text-faint);
	}

	.season input {
		width: 3.2ch;
		padding: 6px 2px;
		border: none;
		background: none;
		font-size: 0.82rem;
		font-variant-numeric: tabular-nums;
		color: var(--text);
		-moz-appearance: textfield;
		appearance: textfield;
	}

	.season input::-webkit-inner-spin-button {
		display: none;
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

	.btn.on {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.row {
		display: grid;
		grid-template-columns: auto 40px minmax(0, 1fr) auto;
		align-items: center;
		gap: 14px;
		padding: 10px 14px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--line);
		background: var(--bg-elev);
	}

	.row.off {
		opacity: 0.45;
	}

	.row.unmatched {
		border-color: color-mix(in srgb, var(--warn) 45%, transparent);
	}

	.row img,
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

	input[type='checkbox'] {
		width: 18px;
		height: 18px;
		accent-color: var(--accent);
	}

	.what {
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

	.sub {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		font-size: 0.75rem;
	}

	.already {
		color: var(--accent);
	}

	.note-line {
		font-size: 0.75rem;
		color: var(--warn);
	}

	.info-line {
		font-size: 0.75rem;
		color: var(--text-faint);
	}

	.candidates {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 4px;
	}

	.cand {
		padding: 3px 9px;
		border-radius: var(--radius-pill);
		border: 1px solid var(--line-strong);
		background: transparent;
		color: var(--text-dim);
		font-size: 0.74rem;
		cursor: pointer;
	}

	.cand:hover {
		color: var(--text);
		border-color: var(--accent);
	}

	.pill.tiny {
		padding: 1px 7px;
		font-size: 0.66rem;
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

	@media (max-width: 760px) {
		.row {
			grid-template-columns: auto 40px minmax(0, 1fr);
			row-gap: 8px;
		}

		.row-controls {
			grid-column: 2 / -1;
		}
	}
</style>
