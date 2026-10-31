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
	import {
		formatBytes,
		formatDuration,
		formatRelative,
		formatTimecode,
		STATUS_LABEL
	} from '../../../../lib/utils/format';
	import { safeFilename, toPlainText } from '../../../../lib/utils/export';

	let transcript = $state<Transcript | null>(null);
	let segments = $state<Segment[]>([]);
	let hasAudio = $state(true);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let copied = $state(false);
	let exporting = $state<ExportFormat | null>(null);
	let handle: JobHandle | null = null;

	const ACTIVE = new Set([
		'uploaded',
		'queued',
		'validating',
		'chunking',
		'transcribing',
		'processing'
	]);

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

	const audioSrc = $derived(transcript && hasAudio ? audioSrcUrl(id, srcNonce) : '');

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
			await getTranscript(id); // refresh-on-401 memperbarui token di memori
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

		// Optimistic.
		segments = segments.map((s) => (s.idx === idx ? { ...s, text } : s));
		try {
			const detail = await patchTranscript(transcript.id, { segments: [{ idx, text }] });
			segments = detail.segments; // sinkron hasil server (trim + edited=true)
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

<div class="mx-auto max-w-6xl px-6">
	<header class="flex items-center justify-between py-5">
		<a href="/dashboard" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<a
			href="/dashboard"
			class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2"
		>
			← Dashboard
		</a>
	</header>

	<main class="pb-12">
		{#if loading && !transcript}
			<div class="space-y-3" aria-label="Memuat">
				<div class="h-8 w-2/3 animate-pulse rounded-lg bg-surface-2"></div>
				<div class="h-64 animate-pulse rounded-xl border border-line bg-surface"></div>
			</div>
		{:else if error && !transcript}
			<div class="rounded-xl border border-danger/30 bg-danger/5 p-8 text-center">
				<p class="font-medium text-danger">{error}</p>
				<a
					href="/dashboard"
					class="mt-4 inline-block rounded-lg bg-accent px-5 py-2.5 text-sm font-medium text-white transition-colors hover:bg-accent-hover"
				>
					Kembali ke dashboard
				</a>
			</div>
		{:else if transcript}
			{#if error}
				<p role="alert" class="mb-4 rounded-lg bg-danger/10 px-4 py-3 text-sm text-danger">{error}</p>
			{/if}

			<div class="mb-6 flex flex-wrap items-start justify-between gap-3">
				<div class="min-w-0 flex-1">
					{#if editingTitle}
						<input
							bind:value={titleDraft}
							onkeydown={(e) => {
								if (e.key === 'Enter') saveTitle();
								if (e.key === 'Escape') (editingTitle = false);
							}}
							onblur={saveTitle}
							class="w-full max-w-xl rounded-lg border border-accent bg-surface px-3 py-1.5 text-2xl font-bold tracking-tight outline-hidden focus:ring-2 focus:ring-accent/30"
							aria-label="Judul transkripsi"
						/>
					{:else}
						<h1 class="flex items-center gap-2 text-2xl font-bold tracking-tight">
							<span class="truncate">{transcript.title}</span>
							{#if isDone}
								<button
									type="button"
									onclick={startTitleEdit}
									class="text-base text-muted transition-colors hover:text-accent"
									aria-label="Ubah judul"
								>
									✎
								</button>
							{/if}
						</h1>
					{/if}
					<p class="mt-1 text-sm text-muted">
						{transcript.source_filename} · {formatBytes(transcript.size_bytes)}
						{#if transcript.duration_seconds}· {formatDuration(transcript.duration_seconds)}{/if}
						· {formatRelative(transcript.created_at)}
						{#if transcript.language}· 🌐 {transcript.language}{/if}
						· {transcript.model}
					</p>
					<p class="mt-1 text-sm">
						Status:
						<span class="font-medium">{STATUS_LABEL[transcript.status] ?? transcript.status}</span>
						{#if ACTIVE.has(transcript.status)}({transcript.progress}%){/if}
					</p>
				</div>
				<div class="flex flex-wrap items-center gap-2">
					<button
						type="button"
						onclick={copyText}
						class="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-hover"
					>
						{copied ? '✓ Tersalin' : 'Salin teks'}
					</button>
					<div class="flex overflow-hidden rounded-lg border border-line">
						{#each ['txt', 'srt', 'vtt', 'json'] as fmt}
							<button
								type="button"
								disabled={!isDone || exporting !== null}
								onclick={() => onExport(fmt as ExportFormat)}
								class="border-l border-line bg-surface px-3 py-2 text-sm font-medium uppercase transition-colors first:border-l-0 hover:bg-surface-2 disabled:opacity-50"
								title={isDone ? `Unduh ${fmt.toUpperCase()}` : 'Tersedia setelah selesai'}
							>
								{exporting === fmt ? '…' : fmt}
							</button>
						{/each}
					</div>
				</div>
			</div>

			{#if ACTIVE.has(transcript.status)}
				<div class="rounded-xl border border-line bg-surface p-8 text-center">
					<div
						class="mx-auto mb-3 flex size-12 items-center justify-center rounded-full bg-accent-soft text-2xl"
						aria-hidden="true"
					>
						⏳
					</div>
					<p class="text-sm text-muted" aria-live="polite">
						{STATUS_LABEL[transcript.status] ?? transcript.status} — halaman ini akan diperbarui
						otomatis saat selesai.
					</p>
					<div class="mx-auto mt-4 h-2.5 max-w-sm overflow-hidden rounded-full bg-surface-2">
						<div
							class="h-full rounded-full bg-accent transition-[width] duration-300"
							style="width: {transcript.progress}%"
						></div>
					</div>
				</div>
			{:else if transcript.status === 'failed'}
				<div class="rounded-xl border border-danger/30 bg-danger/5 p-6">
					<p class="font-medium text-danger">Transkripsi gagal</p>
					<p class="mt-1 text-sm text-danger/90">
						{transcript.error ?? 'Terjadi kesalahan tak diketahui.'}
					</p>
					<button
						type="button"
						onclick={onRetry}
						class="mt-4 rounded-lg border border-warning/40 px-4 py-2 text-sm font-medium text-warning transition-colors hover:bg-warning/10"
					>
						↻ Ulangi transkripsi
					</button>
				</div>
			{:else if transcript.status === 'cancelled'}
				<div class="rounded-xl border border-line bg-surface p-8 text-center">
					<p class="font-medium">Transkripsi dibatalkan</p>
					<p class="mt-1 text-sm text-muted">Unggah ulang audio untuk mencoba lagi.</p>
				</div>
			{:else if isDone && segments.length === 0}
				<div class="rounded-xl border border-line bg-surface p-8 text-center">
					<p class="font-medium">Selesai, tanpa teks terdeteksi</p>
					<p class="mt-1 text-sm text-muted">Audio mungkin tidak berisi ucapan yang jelas.</p>
				</div>
			{:else}
				<div class="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
					<!-- Player (kolom kiri, sticky di layar lebar — docs/07 §3.3) -->
					<div class="lg:sticky lg:top-6 lg:self-start">
						<div class="rounded-xl border border-line bg-surface p-5 shadow-sm">
							{#if hasAudio}
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
								<div class="flex items-center gap-3">
									<button
										type="button"
										onclick={() => skipSegment(-1)}
										class="rounded-lg border border-line px-2.5 py-1.5 text-sm text-muted transition-colors hover:bg-surface-2"
										aria-label="Segment sebelumnya"
									>
										⏮
									</button>
									<button
										type="button"
										onclick={togglePlay}
										class="flex size-11 items-center justify-center rounded-full bg-accent text-lg text-white transition-colors hover:bg-accent-hover"
										aria-label={playing ? 'Jeda' : 'Putar'}
									>
										{playing ? '⏸' : '▶'}
									</button>
									<button
										type="button"
										onclick={() => skipSegment(1)}
										class="rounded-lg border border-line px-2.5 py-1.5 text-sm text-muted transition-colors hover:bg-surface-2"
										aria-label="Segment berikutnya"
									>
										⏭
									</button>
									<span class="ml-1 font-mono text-xs text-muted">
										{formatTimecode(currentTime * 1000)} /
										{formatTimecode((duration || transcript.duration_seconds || 0) * 1000)}
									</span>
								</div>
								<input
									type="range"
									min="0"
									max={Math.max(duration, 0.1)}
									step="0.1"
									value={currentTime}
									oninput={(e) => seekMs(Number((e.target as HTMLInputElement).value) * 1000)}
									class="mt-4 w-full accent-[var(--color-accent)]"
									aria-label="Posisi playback"
								/>
								<div class="mt-4 flex flex-wrap items-center justify-between gap-3 text-sm">
									<div class="flex items-center gap-2">
										<span class="text-muted">Kecepatan</span>
										<select
											value={speed}
											onchange={(e) => setSpeed(Number((e.target as HTMLSelectElement).value))}
											class="rounded-lg border border-line bg-surface px-2 py-1 text-sm"
											aria-label="Kecepatan playback"
										>
											{#each [0.75, 1, 1.25, 1.5, 2] as v}
												<option value={v}>{v}×</option>
											{/each}
										</select>
									</div>
									<label class="flex cursor-pointer items-center gap-2 text-muted">
										<input type="checkbox" bind:checked={follow} class="accent-[var(--color-accent)]" />
										Ikuti segment aktif
									</label>
								</div>
							{:else}
								<p class="text-sm text-muted">
									🎧 Audio sudah dihapus dari penyimpanan (retensi 30 hari) — teks tetap tersedia.
								</p>
							{/if}
							<dl class="mt-5 space-y-1 border-t border-line pt-4 text-sm text-muted">
								{#if transcript.language}
									<div class="flex justify-between">
										<dt>Bahasa</dt>
										<dd class="font-medium text-[var(--color-text)]">{transcript.language}</dd>
									</div>
								{/if}
								<div class="flex justify-between">
									<dt>Model</dt>
									<dd class="font-medium text-[var(--color-text)]">{transcript.model}</dd>
								</div>
								<div class="flex justify-between">
									<dt>Durasi</dt>
									<dd class="font-medium text-[var(--color-text)]">
										{formatDuration(transcript.duration_seconds)}
									</dd>
								</div>
								<div class="flex justify-between">
									<dt>Segment</dt>
									<dd class="font-medium text-[var(--color-text)]">
										{segments.length}{editedCount > 0 ? ` · ${editedCount} diedit` : ''}
									</dd>
								</div>
							</dl>
						</div>
					</div>

					<!-- Editor segment (kolom kanan) -->
					<section aria-label="Daftar segment">
						<h2 class="mb-3 text-sm font-semibold text-muted">
							Transkripsi {isDone ? '· klik teks untuk mengedit' : ''}
						</h2>
						<ul class="space-y-1">
							{#each segments as s (s.idx)}
								<li
									id="seg-{s.idx}"
									class="flex gap-4 rounded-lg px-3 py-2 transition-colors {s.idx ===
									activeIdx
										? 'bg-accent-soft/70 ring-1 ring-accent/40'
										: 'hover:bg-surface-2/60'}"
								>
									<button
										type="button"
										onclick={() => seekMs(s.start_ms + 1)}
										class="shrink-0 pt-0.5 font-mono text-xs text-muted transition-colors hover:text-accent"
										title="Lompat ke {formatTimecode(s.start_ms)}"
									>
										{formatTimecode(s.start_ms)}
									</button>
									<div class="min-w-0 flex-1">
										{#if editingIdx === s.idx}
											<textarea
												bind:value={draft}
												onkeydown={onDraftKey}
												onblur={saveDraft}
												rows="3"
												class="w-full rounded-lg border border-accent bg-surface px-3 py-2 text-sm leading-relaxed outline-hidden focus:ring-2 focus:ring-accent/30"
												aria-label="Edit teks segment"
											></textarea>
											<p class="mt-1 text-xs text-muted">⏎ simpan · Esc batal</p>
										{:else}
											<button
												type="button"
												disabled={!isDone}
												onclick={() => startEdit(s)}
												class="w-full text-left text-sm leading-relaxed transition-colors enabled:hover:text-[var(--color-text)] disabled:cursor-default {s
													.text
													.trim()
													? '' : 'italic text-muted'}"
											>
												{s.text.trim() || '(kosong)'}
											</button>
										{/if}
										{#if s.edited}
											<span class="mt-1 inline-block text-[10px] uppercase tracking-wide text-muted">
												diedit
											</span>
										{/if}
									</div>
								</li>
							{/each}
						</ul>
					</section>
				</div>
			{/if}
		{/if}
	</main>
</div>
