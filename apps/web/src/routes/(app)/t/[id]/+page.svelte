<script lang="ts">
	import { page } from '$app/state';
	import {
		audioSrcUrl,
		downloadExport,
		getTranscript,
		patchTranscript,
		retryTranscript,
		type ExportFormat,
		type Segment,
		type Transcript
	} from '../../../../lib/api/transcripts';
	import { subscribeJob, type JobHandle } from '../../../../lib/api/sse';
	import { formatBytes, formatDuration, formatRelative, formatTimecode } from '../../../../lib/utils/format';
	import { safeFilename, toPlainText } from '../../../../lib/utils/export';
	import { sampleDetail } from '../../../../lib/dev/sample';

	let transcript = $state<Transcript | null>(null);
	let segments = $state<Segment[]>([]);
	let hasAudio = $state(true);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let copied = $state(false);
	let exporting = $state<ExportFormat | null>(null);
	let handle: JobHandle | null = null;

	/** DEV-only (?preview&sample): dataset sampel tanpa API. */
	const sample =
		import.meta.env.DEV &&
		new URLSearchParams(window.location.search).has('sample') &&
		new URLSearchParams(window.location.search).has('preview');

	const ACTIVE = new Set(['uploaded', 'queued', 'validating', 'chunking', 'transcribing', 'processing']);

	// ===== player =====
	let audioEl = $state<HTMLAudioElement | null>(null);
	let currentTime = $state(0);
	let duration = $state(0);
	let playing = $state(false);
	let speed = $state(1);
	let follow = $state(true);
	let srcNonce = $state(0);

	// ===== editor =====
	let editingIdx = $state<number | null>(null);
	let draft = $state('');
	let editingTitle = $state(false);
	let titleDraft = $state('');

	const id = $derived(page.params.id ?? '');
	const fullText = $derived(toPlainText(segments));
	const fileTitle = $derived(transcript?.title ?? 'transkripsi');
	const isDone = $derived(transcript?.status === 'completed');
	const editedCount = $derived(segments.filter((s) => s.edited).length);
	const audioSrc = $derived(transcript && hasAudio && !sample ? audioSrcUrl(id, srcNonce) : '');

	/** Segment aktif = terakhir yang start_ms <= posisi playback. */
	const activeIdx = $derived.by(() => {
		const t = currentTime * 1000;
		let found = -1;
		for (let i = 0; i < segments.length; i++) {
			if (segments[i].start_ms <= t) found = i;
			else break;
		}
		return found;
	});

	async function load() {
		if (!id) return;
		loading = true;
		error = null;
		if (sample) {
			transcript = sampleDetail.transcript as unknown as Transcript;
			segments = sampleDetail.segments;
			hasAudio = false;
			duration = sampleDetail.transcript.duration_seconds ?? 0;
			loading = false;
			return;
		}
		try {
			const detail = await getTranscript(id);
			transcript = detail.transcript;
			segments = detail.segments;
			hasAudio = detail.audio_url !== null;
			watchIfActive();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal memuat transkripsi.';
		} finally {
			loading = false;
		}
	}

	/** Status masih berjalan → pantau via SSE, final → muat hasil (docs/02 §6). */
	function watchIfActive() {
		if (!transcript || !ACTIVE.has(transcript.status) || handle) return;
		handle = subscribeJob(
			transcript.id,
			(ev) => {
				if (transcript && ev.status) {
					transcript.status = ev.status as Transcript['status'];
					transcript.progress = ev.progress ?? transcript.progress;
				}
			},
			() => {
				handle?.stop();
				handle = null;
				load();
			}
		);
	}

	// ===== player wiring =====

	function seekMs(ms: number) {
		if (!audioEl) return;
		audioEl.currentTime = ms / 1000;
		currentTime = ms / 1000;
	}

	function togglePlay() {
		if (!audioEl) return;
		if (audioEl.paused) audioEl.play();
		else audioEl.pause();
	}

	function skipSegment(dir: 1 | -1) {
		if (!segments.length) return;
		const next = Math.min(Math.max(activeIdx + dir, 0), segments.length - 1);
		seekMs(segments[next].start_ms + 1);
	}

	function setSpeed(v: number) {
		speed = v;
		if (audioEl) audioEl.playbackRate = v;
	}

	/** Audio error (mis. token kedaluwarsa) → segarkan token via panggilan API lalu rebuild src. */
	async function onAudioError() {
		if (!audioEl) return;
		try {
			await getTranscript(id);
			srcNonce += 1;
		} catch {
			/* biarkan — player hanya nonaktif */
		}
	}

	// Auto-highlight → ikuti scroll (bisa dijeda via toggle).
	$effect(() => {
		const idx = activeIdx;
		if (!follow || idx < 0 || editingIdx !== null) return;
		document.getElementById(`seg-${idx}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
	});

	// ===== editor =====

	function startEdit(s: Segment) {
		if (!isDone) return;
		editingIdx = s.idx;
		draft = s.text;
	}

	function cancelEdit() {
		editingIdx = null;
		draft = '';
	}

	function onDraftKey(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			saveDraft();
		} else if (e.key === 'Escape') {
			cancelEdit();
		}
	}

	/** Simpan teks segment — optimistic: UI langsung, gagal → rollback (docs/07 §3.3). */
	async function saveDraft() {
		const idx = editingIdx;
		if (idx === null || !transcript) return;
		const text = draft.trim();
		const before = segments.find((s) => s.idx === idx);
		cancelEdit();
		if (!before || text === before.text.trim() || text === '') return;

		segments = segments.map((s) => (s.idx === idx ? { ...s, text } : s));
		if (sample) {
			segments = segments.map((s) => (s.idx === idx ? { ...s, text, edited: true } : s));
			return;
		}
		try {
			const detail = await patchTranscript(transcript.id, { segments: [{ idx, text }] });
			segments = detail.segments;
		} catch (e) {
			segments = segments.map((s) => (s.idx === idx && before ? { ...before } : s));
			error = e instanceof Error ? e.message : 'Gagal menyimpan perubahan.';
		}
	}

	function startTitleEdit() {
		if (!transcript) return;
		editingTitle = true;
		titleDraft = transcript.title;
	}

	async function saveTitle() {
		if (!transcript || !editingTitle) return;
		const title = titleDraft.trim();
		editingTitle = false;
		if (!title || title === transcript.title) return;
		if (sample) {
			transcript.title = title;
			return;
		}
		try {
			const detail = await patchTranscript(transcript.id, { title });
			if (transcript) transcript.title = detail.transcript.title;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal mengubah judul.';
		}
	}

	async function copyText() {
		try {
			await navigator.clipboard.writeText(fullText);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			/* clipboard ditolak browser */
		}
	}

	async function onExport(fmt: ExportFormat) {
		if (!transcript || exporting) return;
		if (sample) return;
		exporting = fmt;
		try {
			await downloadExport(transcript.id, fmt, `${safeFilename(fileTitle)}.${fmt}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Export gagal.';
		} finally {
			exporting = null;
		}
	}

	async function onRetry() {
		if (!transcript) return;
		try {
			await retryTranscript(transcript.id);
			load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Gagal mengulang job.';
		}
	}

	$effect(() => {
		void id;
		load();
		return () => {
			handle?.stop();
			handle = null;
		};
	});
</script>

<div class="mx-auto max-w-6xl px-6 py-8">
	{#if loading && !transcript}
		<div class="space-y-3" aria-label="Memuat">
			<div class="h-7 w-2/3 rounded-sm bg-surface-2"></div>
			<div class="h-3 w-1/2 rounded-sm bg-surface-2"></div>
			<div class="h-72 rounded-(--radius-card) border border-line bg-surface"></div>
		</div>
	{:else if error && !transcript}
		<div class="rounded-(--radius-card) border border-line bg-surface px-6 py-12 text-center">
			<p class="text-sm text-danger">{error}</p>
			<a href="/dashboard" class="mt-3 inline-block text-sm font-medium hover:underline">
				Kembali ke riwayat
			</a>
		</div>
	{:else if transcript}
		{#if error}
			<p role="alert" class="mb-4 rounded-(--radius-control) bg-pill-negative-bg px-4 py-2.5 text-sm text-pill-negative-ink">
				{error}
			</p>
		{/if}

		<!-- Baris judul: judul bisa diedit, meta angka mono, aksi ekspor di kanan -->
		<div class="flex flex-wrap items-start justify-between gap-4">
			<div class="min-w-0">
				{#if editingTitle}
					<input
						bind:value={titleDraft}
						onkeydown={(e) => {
							if (e.key === 'Enter') saveTitle();
							if (e.key === 'Escape') editingTitle = false;
						}}
						onblur={saveTitle}
						class="w-full max-w-xl rounded-(--radius-control) border border-control bg-surface px-3 py-1.5 text-2xl font-semibold tracking-tight outline-hidden focus:border-muted"
						aria-label="Judul transkripsi"
					/>
				{:else}
					<h1 class="flex flex-wrap items-baseline gap-3 text-2xl font-semibold tracking-tight">
						{transcript.title}
						{#if isDone}
							<button type="button" onclick={startTitleEdit} class="text-sm font-normal text-muted hover:text-text hover:underline">
								Ubah judul
							</button>
						{/if}
					</h1>
				{/if}
				<p class="mt-1.5 text-[13px] text-muted">
					{transcript.source_filename}
					· <span class="num">{formatBytes(transcript.size_bytes)}</span>
					{#if transcript.duration_seconds}
						· <span class="num">{formatDuration(transcript.duration_seconds)}</span>
					{/if}
					{#if transcript.language}· bahasa {transcript.language}{/if}
					· {transcript.model}
					· {formatRelative(transcript.created_at)}
				</p>
				{#if ACTIVE.has(transcript.status)}
					<p class="mt-1.5 text-sm text-muted" aria-live="polite">
						Sedang diproses — <span class="num">{transcript.progress}%</span>. Halaman ini diperbarui otomatis saat selesai.
					</p>
				{/if}
			</div>

			<div class="flex items-center gap-2">
				<button type="button" onclick={copyText}
					class="h-9 rounded-(--radius-control) bg-surface-2 px-3.5 text-sm font-medium text-text transition-colors hover:bg-line">
					{copied ? 'Tersalin' : 'Salin teks'}
				</button>
				<div class="flex overflow-hidden rounded-(--radius-control) border border-line">
					{#each ['txt', 'srt', 'vtt', 'json'] as fmt}
						<button
							type="button"
							disabled={!isDone || exporting !== null || sample}
							onclick={() => onExport(fmt as ExportFormat)}
							class="border-l border-line bg-surface px-3 text-sm font-medium uppercase transition-colors first:border-l-0 hover:bg-surface-2 enabled:hover:bg-surface-2 disabled:text-muted disabled:bg-surface-2/50"
							title={isDone ? `Unduh ${fmt.toUpperCase()}` : 'Tersedia setelah selesai'}
						>
							{exporting === fmt ? '…' : fmt}
						</button>
					{/each}
				</div>
			</div>
		</div>

		{#if ACTIVE.has(transcript.status)}
			<div class="mt-8 rounded-(--radius-card) border border-line bg-surface px-6 py-12 text-center">
				<p class="text-sm text-muted">Menunggu hasil — tidak ada teks yang bisa diedit sebelum selesai.</p>
			</div>
		{:else if transcript.status === 'failed'}
			<div class="mt-8 rounded-(--radius-card) border border-line bg-surface px-6 py-8">
				<p class="font-medium text-danger">Transkripsi gagal</p>
				<p class="mt-1 text-sm text-muted">{transcript.error ?? 'Terjadi kesalahan tak diketahui.'}</p>
				<button type="button" onclick={onRetry}
					class="mt-4 h-9 rounded-(--radius-control) bg-surface-2 px-4 text-sm font-medium transition-colors hover:bg-line">
					Ulangi transkripsi
				</button>
			</div>
		{:else if transcript.status === 'cancelled'}
			<div class="mt-8 rounded-(--radius-card) border border-line bg-surface px-6 py-12 text-center">
				<p class="font-medium">Transkripsi dibatalkan</p>
				<p class="mt-1 text-sm text-muted">Unggah ulang audio untuk mencoba lagi.</p>
			</div>
		{:else if isDone && segments.length === 0}
			<div class="mt-8 rounded-(--radius-card) border border-line bg-surface px-6 py-12 text-center">
				<p class="font-medium">Selesai, tanpa teks terdeteksi</p>
				<p class="mt-1 text-sm text-muted">Audio mungkin tidak berisi ucapan yang jelas.</p>
			</div>
		{:else}
			<div class="mt-8 grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
				<!-- Player — hue data di posisi playback -->
				<div class="lg:sticky lg:top-18 lg:self-start">
					<div class="rounded-(--radius-card) border border-line bg-surface p-5">
						{#if audioSrc}
							<audio
								bind:this={audioEl}
								src={audioSrc}
								preload="metadata"
								ontimeupdate={() => (currentTime = audioEl?.currentTime ?? 0)}
								ondurationchange={() => (duration = audioEl?.duration ?? 0)}
								onplay={() => (playing = true)}
								onpause={() => (playing = false)}
								onerror={onAudioError}
								class="hidden"
							></audio>
						{/if}
						<div class="flex items-center gap-3">
							<button type="button" onclick={() => skipSegment(-1)}
								class="h-9 rounded-(--radius-control) border border-line px-3 text-sm text-muted transition-colors hover:bg-surface-2"
								aria-label="Segment sebelumnya">
								⏮
							</button>
							<button type="button" onclick={togglePlay}
								class="flex size-11 items-center justify-center rounded-full bg-accent text-white transition-colors hover:bg-accent-hover"
								aria-label={playing ? 'Jeda' : 'Putar'}>
								{playing ? '⏸' : '▶'}
							</button>
							<button type="button" onclick={() => skipSegment(1)}
								class="h-9 rounded-(--radius-control) border border-line px-3 text-sm text-muted transition-colors hover:bg-surface-2"
								aria-label="Segment berikutnya">
								⏭
							</button>
							<span class="num ml-auto text-xs text-muted">
								{formatTimecode(currentTime * 1000)} /
								{formatTimecode((duration || transcript.duration_seconds || 0) * 1000)}
							</span>
						</div>

						<input type="range" min="0" max={Math.max(duration, 0.1)} step="0.1" value={currentTime}
							oninput={(e) => seekMs(Number((e.target as HTMLInputElement).value) * 1000)}
							class="mt-4 w-full accent-data"
							aria-label="Posisi playback" />

						<div class="mt-4 flex flex-wrap items-center justify-between gap-3 text-sm">
							<label class="flex items-center gap-2 text-muted">
								Kecepatan
								<select value={speed} onchange={(e) => setSpeed(Number((e.target as HTMLSelectElement).value))}
									class="h-8 rounded-(--radius-control) border border-control bg-surface px-2 text-sm">
									{#each [0.75, 1, 1.25, 1.5, 2] as v}
										<option value={v}>{v}×</option>
									{/each}
								</select>
							</label>
							<label class="flex cursor-pointer items-center gap-2 text-muted">
								<input type="checkbox" bind:checked={follow} class="accent-data" />
								Ikuti segment aktif
							</label>
						</div>

						<dl class="mt-5 space-y-1 border-t border-rule pt-4 text-sm">
							{#if transcript.language}
								<div class="flex h-8 items-center justify-between">
									<dt class="text-muted">Bahasa terdeteksi</dt>
									<dd>{transcript.language}</dd>
								</div>
							{/if}
							<div class="flex h-8 items-center justify-between">
								<dt class="text-muted">Model</dt>
								<dd>{transcript.model}</dd>
							</div>
							<div class="flex h-8 items-center justify-between">
								<dt class="text-muted">Durasi</dt>
								<dd class="num">{formatDuration(transcript.duration_seconds)}</dd>
							</div>
							<div class="flex h-8 items-center justify-between">
								<dt class="text-muted">Segment</dt>
								<dd class="num">
									{segments.length}{editedCount > 0 ? ` · ${editedCount} diedit` : ''}
								</dd>
							</div>
						</dl>
					</div>
				</div>

				<!-- Ledger segment: timecode mono + teks; aktif = hue data -->
				<section aria-label="Transkripsi per segment">
					<h2 class="mb-3 text-sm font-medium text-muted">
						Transkripsi{isDone ? ' — klik teks untuk mengedit' : ''}
					</h2>
					<ul class="divide-y divide-rule rounded-(--radius-card) border border-line bg-surface">
						{#each segments as s (s.idx)}
							<li id="seg-{s.idx}" class="relative flex gap-4 px-4 py-3 transition-colors {s.idx === activeIdx
								? 'bg-data-soft'
								: 'hover:bg-surface-2/50'}">
								{#if s.idx === activeIdx}
									<span class="absolute inset-y-0 left-0 w-0.5 bg-data" aria-hidden="true"></span>
								{/if}
								<button type="button" onclick={() => seekMs(s.start_ms + 1)}
									class="num w-14 shrink-0 pt-0.5 text-right text-xs text-muted transition-colors hover:text-data"
									title="Lompat ke {formatTimecode(s.start_ms)}">
									{formatTimecode(s.start_ms)}
								</button>
								<div class="min-w-0 flex-1">
									{#if editingIdx === s.idx}
										<textarea bind:value={draft} onkeydown={onDraftKey} onblur={saveDraft} rows="3"
											class="w-full rounded-(--radius-control) border border-control bg-surface px-3 py-2 text-sm leading-relaxed outline-hidden focus:border-muted"
											aria-label="Edit teks segment"></textarea>
										<p class="mt-1 text-xs text-muted">Enter simpan · Esc batal</p>
									{:else}
										<button type="button" disabled={!isDone} onclick={() => startEdit(s)}
											class="w-full text-left text-sm leading-relaxed transition-colors enabled:hover:text-text {s.text.trim() ? '' : 'italic text-muted'}">
											{s.text.trim() || '(kosong)'}
										</button>
									{/if}
									{#if s.edited}
										<span class="mt-1 inline-block text-xs text-muted">diedit</span>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				</section>
			</div>
		{/if}
	{/if}
</div>
