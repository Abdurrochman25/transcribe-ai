import { API_BASE, api, getAccessToken } from './client';
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
