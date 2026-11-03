<script lang="ts">
	import { uploadTranscript, type Transcript } from '../../../lib/api/transcripts';
	import { subscribeJob, type JobEvent } from '../../../lib/api/sse';
	import { ApiError } from '../../../lib/api/types';
	import { formatDuration } from '../../../lib/utils/format';

	const ALLOWED_EXT = ['mp3', 'wav', 'm4a', 'mp4', 'ogg', 'oga', 'opus', 'flac', 'webm'];
	const MAX_BYTES = 200 * 1024 * 1024;

	let file = $state<File | null>(null);
	let title = $state('');
	let language = $state(''); // '' = auto-detect
	let model = $state('whisper-large-v3-turbo');
	let prompt = $state('');

	let dragging = $state(false);
	let uploading = $state(false);
	let uploadProgress = $state(0);
	let error = $state<string | null>(null);
	let done = $state<Transcript | null>(null);

	// Progres job (SSE) setelah upload diterima.
	let jobEvent = $state<JobEvent | null>(null);
	let jobDone = $state(false);
	let handle = $state<ReturnType<typeof subscribeJob> | null>(null);

	const fileLabel = $derived(
		file ? `${file.name} · ${(file.size / 1024 / 1024).toFixed(1)} MB` : null
	);
	const jobStatusText = $derived(
		jobDone
			? 'Selesai.'
			: jobEvent?.message ??
				({ validating: 'Memeriksa file…', chunking: 'Memotong audio…', transcribing: 'Mentranskripsi…', processing: 'Memfinalisasi…', queued: 'Menunggu antrean…' })[jobEvent?.status ?? 'queued'] ??
				'Memproses…'
	);

	function pick(f: File | undefined | null) {
		error = null;
		if (!f) return;
		const ext = f.name.split('.').pop()?.toLowerCase() ?? '';
		if (!ALLOWED_EXT.includes(ext)) {
			error = `Format .${ext} tidak didukung. Gunakan: ${ALLOWED_EXT.join(', ')}.`;
			return;
		}
		if (f.size > MAX_BYTES) {
			error = 'Ukuran file melebihi 200 MB.';
			return;
		}
		file = f;
		if (!title) title = f.name.replace(/\.[^.]+$/, '');
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		dragging = false;
		pick(e.dataTransfer?.files?.[0]);
	}

	function onFileInput(e: Event) {
		const input = e.target as HTMLInputElement;
		pick(input.files?.[0]);
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!file || uploading) return;
		error = null;
		uploading = true;
		uploadProgress = 0;
		try {
			done = await uploadTranscript({
				file,
				title: title.trim() || undefined,
				language: language || undefined,
				model,
				prompt: prompt.trim() || undefined,
				onProgress: (f) => (uploadProgress = f)
			});
			// Pantau job via SSE sampai final (docs/02 §6).
			handle = subscribeJob(
				done.id,
				(ev) => (jobEvent = ev),
				() => {
					jobDone = true;
					handle?.stop();
				}
			);
		} catch (err) {
			if (err instanceof ApiError) {
				const detail = err.details?.map((d) => `${d.field}: ${d.issue}`).join(' · ');
				error = detail ? `${err.message} (${detail})` : err.message;
			} else {
				error = 'Tidak bisa menghubungi server. Coba lagi.';
			}
		} finally {
			uploading = false;
		}
	}

	function reset() {
		handle?.stop();
		file = null;
		title = '';
		prompt = '';
		uploadProgress = 0;
		done = null;
		jobEvent = null;
		jobDone = false;
	}
</script>

<div class="mx-auto max-w-2xl px-6 py-8">
	<div class="mb-6">
		<h1 class="text-2xl font-semibold tracking-tight">Upload audio</h1>
		<p class="mt-1 text-sm text-muted">
			{ALLOWED_EXT.map((e) => e.toUpperCase()).join(', ')} · maksimal 200 MB per rekaman.
		</p>
	</div>

	{#if done}
		<!-- Sukses upload → pantau job live; hue di progres (data), angka mono -->
		<div class="rounded-[var(--radius-card)] border border-line bg-surface p-8">
			<h2 class="text-lg font-semibold">{done.title}</h2>

			{#if !jobDone}
				<p class="mt-1 text-sm text-muted" aria-live="polite">{jobStatusText}</p>
				<div class="mt-4 max-w-sm">
					<div class="mb-1 flex justify-between text-xs text-muted">
						<span>Progres transkripsi</span>
						<span class="num">{jobEvent?.progress ?? 0}%</span>
					</div>
					<div class="h-1 w-full overflow-hidden rounded-full bg-line">
						<div
							class="h-full rounded-full bg-data transition-[width] duration-300"
							style="width: {jobEvent?.progress ?? 0}%"
						></div>
					</div>
				</div>
				<p class="mt-3 text-xs text-muted">
					Halaman ini boleh ditinggalkan — progres berlanjut di riwayat.
				</p>
			{:else if jobEvent?.type === 'completed'}
				<p class="mt-1 text-sm">
					Transkripsi selesai{jobEvent?.language ? ` · bahasa ${jobEvent.language}` : ''}
					{done.duration_seconds ? ` · <span class="num">${formatDuration(done.duration_seconds)}</span>` : ''}
				</p>
			{:else}
				<p class="mt-1 text-sm text-danger">
					{jobEvent?.message || 'Job berhenti sebelum selesai.'}
				</p>
			{/if}

			<div class="mt-6 flex gap-3">
				<a
					href={jobEvent?.type === 'completed' ? `/t/${done.id}` : '/dashboard'}
					class="h-9 rounded-[var(--radius-control)] bg-accent px-4 text-sm font-medium leading-9 text-white transition-colors hover:bg-accent-hover"
				>
					{jobEvent?.type === 'completed' ? 'Lihat hasil' : 'Lihat riwayat'}
				</a>
				<button
					type="button"
					onclick={reset}
					class="h-9 rounded-[var(--radius-control)] bg-surface-2 px-4 text-sm font-medium transition-colors hover:bg-line"
				>
					Upload lagi
				</button>
			</div>
		</div>
	{:else}
		<form onsubmit={submit} class="space-y-5">
			<!-- Dropzone: slot kosong (garis putus) + mark gelombang -->
			<div
				role="button"
				tabindex="0"
				aria-label="Pilih atau tarik file audio ke sini"
				class="flex cursor-pointer flex-col items-center justify-center rounded-[var(--radius-card)] border border-dashed px-6 py-12 text-center transition-colors {dragging
					? 'border-muted bg-surface-2/60'
					: 'border-control bg-surface hover:border-muted'}"
				ondragover={(e) => {
					e.preventDefault();
					dragging = true;
				}}
				ondragleave={() => (dragging = false)}
				ondrop={onDrop}
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') document.getElementById('file-input')?.click();
				}}
				onclick={() => document.getElementById('file-input')?.click()}
			>
				<svg class="mb-3 text-muted" width="24" height="24" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<rect x="1" y="5" width="2" height="6" rx="1" fill="currentColor" />
					<rect x="4.25" y="3" width="2" height="10" rx="1" fill="currentColor" />
					<rect x="7.5" y="1" width="2" height="14" rx="1" fill="currentColor" />
					<rect x="10.75" y="4" width="2" height="8" rx="1" fill="currentColor" />
					<rect x="14" y="6" width="2" height="4" rx="1" fill="currentColor" />
				</svg>
				{#if file}
					<p class="font-medium">{fileLabel}</p>
					<p class="mt-1 text-xs text-muted">Klik untuk mengganti file</p>
				{:else}
					<p class="font-medium">Tarik &amp; lepas file audio di sini</p>
					<p class="mt-1 text-sm text-muted">atau klik untuk memilih dari komputermu</p>
				{/if}
				<input id="file-input" type="file" class="sr-only" accept={ALLOWED_EXT.map((e) => '.' + e).join(',')} onchange={onFileInput} />
			</div>

			{#if error}
				<p role="alert" class="rounded-[var(--radius-control)] bg-pill-negative-bg px-3 py-2 text-sm text-pill-negative-ink">
					{error}
				</p>
			{/if}

			<!-- Opsi: label di atas field, input putih border control -->
			<div class="space-y-4 rounded-[var(--radius-card)] border border-line bg-surface p-5">
				<div>
					<label for="title" class="mb-1.5 block text-sm font-medium">Judul</label>
					<input id="title" type="text" bind:value={title} placeholder="Otomatis dari nama file"
						class="h-10 w-full rounded-[var(--radius-control)] border border-control bg-surface px-3 text-sm outline-hidden placeholder:text-faint focus:border-muted" />
				</div>
				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="language" class="mb-1.5 block text-sm font-medium">Bahasa</label>
						<select id="language" bind:value={language}
							class="h-10 w-full rounded-[var(--radius-control)] border border-control bg-surface px-3 text-sm outline-hidden focus:border-muted">
							<option value="">Deteksi otomatis</option>
							<option value="id">Indonesia</option>
							<option value="en">Inggris</option>
							<option value="jv">Jawa</option>
							<option value="su">Sunda</option>
							<option value="ar">Arab</option>
							<option value="ms">Melayu</option>
							<option value="zh">Mandarin</option>
							<option value="ja">Jepang</option>
							<option value="ko">Korea</option>
						</select>
					</div>
					<div>
						<label for="model" class="mb-1.5 block text-sm font-medium">Model</label>
						<select id="model" bind:value={model}
							class="h-10 w-full rounded-[var(--radius-control)] border border-control bg-surface px-3 text-sm outline-hidden focus:border-muted">
							<option value="whisper-large-v3-turbo">whisper-large-v3-turbo (cepat)</option>
							<option value="whisper-large-v3">whisper-large-v3 (akurat)</option>
						</select>
					</div>
				</div>
				<div>
					<label for="prompt" class="mb-1.5 block text-sm font-medium">
						Petunjuk AI <span class="font-normal text-muted">(opsional)</span>
					</label>
					<textarea id="prompt" rows="2" bind:value={prompt} maxlength="500"
						placeholder="Contoh: nama dosen Dr. Rani, istilah: RAG, LLM, fine-tuning"
						class="w-full rounded-[var(--radius-control)] border border-control bg-surface px-3 py-2.5 text-sm outline-hidden placeholder:text-faint focus:border-muted"></textarea>
				</div>
			</div>

			<!-- Progres upload: hue data, angka mono -->
			{#if uploading}
				<div aria-live="polite">
					<div class="mb-1 flex justify-between text-sm">
						<span>Mengunggah…</span>
						<span class="num text-muted">{Math.round(uploadProgress * 100)}%</span>
					</div>
					<div class="h-1 w-full overflow-hidden rounded-full bg-line">
						<div class="h-full rounded-full bg-data transition-[width] duration-200" style="width: {uploadProgress * 100}%"></div>
					</div>
				</div>
			{/if}

			<div class="flex items-center justify-end gap-4">
				<a href="/dashboard" class="text-sm text-muted transition-colors hover:text-text">Batal</a>
				<button type="submit" disabled={!file || uploading}
					class="h-10 rounded-[var(--radius-control)] bg-accent px-5 text-sm font-medium text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40">
					{uploading ? 'Mengunggah…' : 'Mulai transkripsi'}
				</button>
			</div>
		</form>
	{/if}
</div>
