/**
 * Formatter export transcript — murni frontend (bagian awal docs/03 §export).
 * Versi lengkap (backend + header download) datang di Fase 4.
 */

import type { Segment } from '../api/transcripts';

function pad(n: number, len = 2): string {
	return String(n).padStart(len, '0');
}

function srtTimecode(ms: number): string {
	const h = Math.floor(ms / 3_600_000);
	const m = Math.floor(ms / 60_000) % 60;
	const s = Math.floor(ms / 1000) % 60;
	const frac = ms % 1000;
	return `${pad(h)}:${pad(m)}:${pad(s)},${pad(frac, 3)}`;
}

function vttTimecode(ms: number): string {
	return srtTimecode(ms).replace(',', '.');
}

export function toPlainText(segments: Segment[]): string {
	return segments.map((s) => s.text.trim()).join('\n\n');
}

export function toSrt(segments: Segment[]): string {
	return segments
		.map(
			(s, i) =>
				`${i + 1}\n${srtTimecode(s.start_ms)} --> ${srtTimecode(s.end_ms)}\n${s.text.trim()}\n`
		)
		.join('\n');
}

export function toVtt(segments: Segment[]): string {
	const cues = segments
		.map((s) => `${vttTimecode(s.start_ms)} --> ${vttTimecode(s.end_ms)}\n${s.text.trim()}\n`)
		.join('\n');
	return `WEBVTT\n\n${cues}`;
}

/** Nama file aman dari judul transcript. */
export function safeFilename(title: string, fallback = 'transkripsi'): string {
	const clean = title.replace(/[\\/:*?"<>|]+/g, '').replace(/\s+/g, ' ').trim();
	return clean || fallback;
}

/** Picu unduhan file teks di browser (blob lokal, tanpa server). */
export function downloadText(filename: string, content: string, mime = 'text/plain'): void {
	const blob = new Blob([content], { type: `${mime};charset=utf-8` });
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = filename;
	a.click();
	URL.revokeObjectURL(url);
}
