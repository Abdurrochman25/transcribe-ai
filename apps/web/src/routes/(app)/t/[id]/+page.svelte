<script lang="ts">
	import { page } from '$app/state';
	import {
		getTranscript,
		retryTranscript,
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
	import {
		downloadText,
		safeFilename,
		toPlainText,
		toSrt,
		toVtt
	} from '../../../../lib/utils/export';

	let transcript = $state<Transcript | null>(null);
	let segments = $state<Segment[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let copied = $state(false);
	let handle: JobHandle | null = null;

	const ACTIVE = new Set([
		'uploaded',
		'queued',
		'validating',
		'chunking',
		'transcribing',
		'processing'
	]);

	const id = $derived(page.params.id);
	const fullText = $derived(toPlainText(segments));
	const isDone = $derived(transcript?.status === 'completed');
	// Closure handler tidak mewarisi narrowing template → nilai turunan via derived.
	const fileTitle = $derived(transcript?.title ?? 'transkripsi');

	async function load() {
		if (!id) return;
		loading = true;
		error = null;
		try {
			const detail = await getTranscript(id);
			transcript = detail.transcript;
			segments = detail.segments;
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

	async function copyText() {
		try {
			await navigator.clipboard.writeText(fullText);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			/* clipboard ditolak browser */
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

<div class="mx-auto max-w-4xl px-6">
	<header class="flex items-center justify-between py-5">
		<a href="/dashboard" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<a href="/dashboard" class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2">
			← Dashboard
		</a>
	</header>

	<main class="pb-12">
		{#if loading && !transcript}
			<div class="space-y-3" aria-label="Memuat">
				<div class="h-8 w-2/3 animate-pulse rounded-lg bg-surface-2"></div>
				<div class="h-40 animate-pulse rounded-xl border border-line bg-surface"></div>
			</div>
		{:else if error && !transcript}
			<div class="rounded-xl border border-danger/30 bg-danger/5 p-8 text-center">
				<p class="font-medium text-danger">{error}</p>
				<a href="/dashboard" class="mt-4 inline-block rounded-lg bg-accent px-5 py-2.5 text-sm font-medium text-white transition-colors hover:bg-accent-hover">
					Kembali ke dashboard
				</a>
			</div>
		{:else if transcript}
			<div class="mb-6">
				<h1 class="text-2xl font-bold tracking-tight">{transcript.title}</h1>
				<p class="mt-1 text-sm text-muted">
					{transcript.source_filename} · {formatBytes(transcript.size_bytes)}
					{#if transcript.duration_seconds}· {formatDuration(transcript.duration_seconds)}{/if}
					· {formatRelative(transcript.created_at)}
					{#if transcript.language}· 🌐 {transcript.language}{/if}
					· {transcript.model}
				</p>
				<p class="mt-2 text-sm">
					Status:
					<span class="font-medium">{STATUS_LABEL[transcript.status] ?? transcript.status}</span>
					{#if ACTIVE.has(transcript.status)}({transcript.progress}%){/if}
				</p>
			</div>

			{#if ACTIVE.has(transcript.status)}
				<div class="rounded-xl border border-line bg-surface p-8 text-center">
					<div class="mx-auto mb-3 flex size-12 items-center justify-center rounded-full bg-accent-soft text-2xl" aria-hidden="true">⏳</div>
					<p class="text-sm text-muted" aria-live="polite">
						{STATUS_LABEL[transcript.status] ?? transcript.status} — halaman ini akan diperbarui otomatis saat selesai.
					</p>
					<div class="mx-auto mt-4 h-2.5 max-w-sm overflow-hidden rounded-full bg-surface-2">
						<div class="h-full rounded-full bg-accent transition-[width] duration-300" style="width: {transcript.progress}%"></div>
					</div>
				</div>
			{:else if transcript.status === 'failed'}
				<div class="rounded-xl border border-danger/30 bg-danger/5 p-6">
					<p class="font-medium text-danger">Transkripsi gagal</p>
					<p class="mt-1 text-sm text-danger/90">{transcript.error ?? 'Terjadi kesalahan tak diketahui.'}</p>
					<button type="button" onclick={onRetry}
						class="mt-4 rounded-lg border border-warning/40 px-4 py-2 text-sm font-medium text-warning transition-colors hover:bg-warning/10">
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
				<div class="mb-6 flex flex-wrap items-center gap-2">
					<button type="button" onclick={copyText}
						class="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-hover">
						{copied ? '✓ Tersalin' : 'Salin teks'}
					</button>
					<button type="button"
						onclick={() => downloadText(`${safeFilename(fileTitle)}.txt`, fullText)}
						class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2">
						⬇ TXT
					</button>
					<button type="button"
						onclick={() => downloadText(`${safeFilename(fileTitle)}.srt`, toSrt(segments), 'application/x-subrip')}
						class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2">
						⬇ SRT
					</button>
					<button type="button"
						onclick={() => downloadText(`${safeFilename(fileTitle)}.vtt`, toVtt(segments), 'text/vtt')}
						class="rounded-lg border border-line bg-surface px-4 py-2 text-sm font-medium transition-colors hover:bg-surface-2">
						⬇ VTT
					</button>
				</div>

				<section class="mb-6 rounded-xl border border-line bg-surface p-6 shadow-sm" aria-label="Teks lengkap">
					<h2 class="mb-3 text-sm font-semibold text-muted">Teks lengkap</h2>
					<p class="whitespace-pre-line leading-relaxed">{fullText}</p>
				</section>

				<section aria-label="Daftar segment">
					<h2 class="mb-3 text-sm font-semibold text-muted">Per segment ({segments.length})</h2>
					<ul class="space-y-1">
						{#each segments as s (s.idx)}
							<li class="flex gap-4 rounded-lg px-3 py-2 transition-colors hover:bg-surface-2/60">
								<span class="shrink-0 pt-0.5 font-mono text-xs text-muted" title="{formatTimecode(s.start_ms)} – {formatTimecode(s.end_ms)}">
									{formatTimecode(s.start_ms)}
								</span>
								<span class="min-w-0">{s.text.trim()}</span>
							</li>
						{/each}
					</ul>
					<p class="mt-4 text-xs text-muted">
						Editor segment &amp; player audio sinkron menyusul di Fase 4 — data timestamp sudah siap.
					</p>
				</section>
			{/if}
		{/if}
	</main>
</div>
