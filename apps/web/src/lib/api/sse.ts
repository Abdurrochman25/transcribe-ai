/**
 * Subscribe progres job via SSE (`/transcripts/{id}/events`) dengan fallback
 * polling GET detail tiap 3 detik (docs/06 §3, docs/02 §6).
 * EventSource tidak bisa kirim Authorization → token via query param (TTL 15m).
 */

import { API_BASE, api, getAccessToken } from './client';
import type { Transcript } from './transcripts';

export interface JobEvent {
	type: 'progress' | 'completed' | 'failed' | 'cancelled';
	status?: string;
	progress?: number;
	message?: string | null;
	transcript_id?: string;
	language?: string | null;
}

export interface JobHandle {
	stop: () => void;
}

const FINAL = new Set(['completed', 'failed', 'cancelled']);

export function subscribeJob(
	id: string,
	onEvent: (event: JobEvent) => void,
	onFinal: () => void
): JobHandle {
	const token = getAccessToken();
	let closed = false;
	let poller: ReturnType<typeof setInterval> | null = null;

	const stop = () => {
		closed = true;
		es?.close();
		if (poller) clearInterval(poller);
	};

	const handle = (event: JobEvent) => {
		onEvent(event);
		if (FINAL.has(event.type)) {
			stop();
			onFinal();
		}
	};

	// Fallback polling (proxy blokir SSE / EventSource tak tersedia).
	const startPolling = () => {
		if (closed || poller) return;
		poller = setInterval(async () => {
			try {
				const body = await api<{ transcript: Transcript }>(`/transcripts/${id}`);
				const t = body.transcript;
				if (FINAL.has(t.status)) {
					handle({
						type: t.status as JobEvent['type'],
						status: t.status,
						progress: t.progress,
						message: t.error ?? null
					});
				} else {
					onEvent({ type: 'progress', status: t.status, progress: t.progress });
				}
			} catch {
				/* coba lagi interval berikutnya */
			}
		}, 3000);
	};

	let es: EventSource | null = null;
	if (token && typeof EventSource !== 'undefined') {
		es = new EventSource(`${API_BASE}/transcripts/${id}/events?token=${encodeURIComponent(token)}`);
		for (const kind of ['progress', 'completed', 'failed', 'cancelled'] as const) {
			es.addEventListener(kind, (ev) => {
				let data: Record<string, unknown> = {};
				try {
					data = JSON.parse((ev as MessageEvent).data) as Record<string, unknown>;
				} catch {
					/* data tidak valid — pakai default */
				}
				handle({
					type: kind,
					status: typeof data.status === 'string' ? data.status : undefined,
					progress: typeof data.progress === 'number' ? data.progress : undefined,
					message: typeof data.message === 'string' ? data.message : null,
					transcript_id: typeof data.transcript_id === 'string' ? data.transcript_id : id,
					language: typeof data.language === 'string' ? data.language : null
				});
			});
		}
		es.onerror = () => {
			// Koneksi SSE bermasalah → fallback polling (idempotent).
			es?.close();
			es = null;
			startPolling();
		};
	} else {
		startPolling();
	}

	return { stop };
}
