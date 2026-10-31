import { API_BASE, api, apiBlob, getAccessToken } from './client';
import { ApiError, type FieldError } from './types';

export type TranscriptStatus =
	| 'uploaded'
	| 'queued'
	| 'validating'
	| 'chunking'
	| 'transcribing'
	| 'processing'
	| 'completed'
	| 'failed'
	| 'cancelled';

export interface Transcript {
	id: string;
	title: string;
	status: TranscriptStatus;
	progress: number;
	source_filename: string;
	size_bytes: number;
	mime_type: string;
	duration_seconds: number | null;
	language: string | null;
	requested_language: string | null;
	model: string;
	error?: string | null;
	created_at: string;
	completed_at?: string;
}

export interface TranscriptList {
	items: Transcript[];
	page: number;
	per_page: number;
	total: number;
	total_pages: number;
}

export interface ListParams {
	page?: number;
	per_page?: number;
	status?: string;
	q?: string;
}

export async function listTranscripts(params: ListParams = {}): Promise<TranscriptList> {
	const search = new URLSearchParams();
	if (params.page) search.set('page', String(params.page));
	if (params.per_page) search.set('per_page', String(params.per_page));
	if (params.status) search.set('status', params.status);
	if (params.q) search.set('q', params.q);
	const qs = search.toString();
	return api<TranscriptList>(`/transcripts${qs ? `?${qs}` : ''}`);
}

export async function cancelTranscript(id: string): Promise<void> {
	await api<void>(`/transcripts/${id}/cancel`, { method: 'POST' });
}

export async function retryTranscript(id: string): Promise<void> {
	await api<void>(`/transcripts/${id}/retry`, { method: 'POST' });
}

/** Hasil transkripsi per segment (docs/03 — hanya tersedia setelah completed). */
export interface Segment {
	idx: number;
	start_ms: number;
	end_ms: number;
	text: string;
	confidence: number | null;
	edited: boolean;
}

export interface TranscriptDetail {
	transcript: Transcript;
	segments: Segment[];
	/** Null bila audio sudah dihapus retensi/cancel (docs/03). */
	audio_url: string | null;
}

export async function getTranscript(id: string): Promise<TranscriptDetail> {
	return api<TranscriptDetail>(`/transcripts/${id}`);
}

// ===== editor (fase 4) =====

export interface PatchSegmentInput {
	idx: number;
	text: string;
}

export interface PatchInput {
	title?: string;
	segments?: PatchSegmentInput[];
}

export async function patchTranscript(id: string, input: PatchInput): Promise<TranscriptDetail> {
	return api<TranscriptDetail>(`/transcripts/${id}`, { method: 'PATCH', body: input });
}

/** URL elemen `<audio>` — endpoint streaming; token via query karena elemen
 *  tidak bisa kirim header Authorization (TTL 15 mnt, auto-segar saat error). */
export function audioSrcUrl(id: string, nonce = 0): string {
	const token = getAccessToken();
	const q = token ? `?token=${encodeURIComponent(token)}` : '';
	return `${API_BASE}/transcripts/${id}/audio${q}${nonce ? `${token ? '&' : '?'}n=${nonce}` : ''}`;
}

export type ExportFormat = 'txt' | 'srt' | 'vtt' | 'json';

/** Unduh export dari backend (blob via fetch ber-auth → anchor download). */
export async function downloadExport(id: string, format: ExportFormat, filename: string): Promise<void> {
	const blob = await apiBlob(`/transcripts/${id}/export?format=${format}`);
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = filename;
	a.click();
	URL.revokeObjectURL(url);
}

export interface UploadOptions {
	file: File;
	title?: string;
	language?: string;
	model?: string;
	prompt?: string;
	onProgress?: (fraction: number) => void;
}

/**
 * Upload via XHR untuk progress byte-level (fetch belum mendukung upload progress).
 * Access token diambil dari memori client; cookie refresh tetap terkirim (withCredentials).
 */
export function uploadTranscript(opts: UploadOptions): Promise<Transcript> {
	return new Promise((resolve, reject) => {
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `${API_BASE}/transcripts`);
		xhr.withCredentials = true;

		const token = getAccessToken();
		if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);

		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable && opts.onProgress) {
				opts.onProgress(e.loaded / e.total);
			}
		};

		xhr.onload = () => {
			if (xhr.status === 202) {
				try {
					const body = JSON.parse(xhr.responseText) as { transcript: Transcript };
					resolve(body.transcript);
				} catch {
					reject(new ApiError(xhr.status, 'INTERNAL', 'Respons server tidak valid'));
				}
				return;
			}
			// Normalisasi envelope error — bentuk sama dengan fetch client.
			try {
				const body = JSON.parse(xhr.responseText) as {
					error?: { code?: string; message?: string; details?: FieldError[]; request_id?: string };
				};
				reject(
					new ApiError(
						xhr.status,
						body.error?.code ?? 'INTERNAL',
						body.error?.message ?? 'Upload gagal',
						body.error?.details,
						body.error?.request_id
					)
				);
			} catch {
				reject(new ApiError(xhr.status, 'INTERNAL', `Upload gagal (${xhr.status})`));
			}
		};

		xhr.onerror = () => reject(new ApiError(0, 'NETWORK', 'Tidak bisa menghubungi server'));

		const form = new FormData();
		form.append('file', opts.file);
		if (opts.title) form.append('title', opts.title);
		if (opts.language) form.append('language', opts.language);
		if (opts.model) form.append('model', opts.model);
		if (opts.prompt) form.append('prompt', opts.prompt);

		xhr.send(form);
	});
}
