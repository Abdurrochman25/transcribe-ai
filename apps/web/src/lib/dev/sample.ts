/**
 * Dataset sampel DEV-ONLY untuk inspeksi desain layar auth (?preview&sample).
 * Jangan dipakai di produksi — tree-shaken saat build prod (import.meta.env.DEV guard).
 *
 * Kejujuran data (app-screens §2.1): total dihitung dari baris, angka kecil
 * tetap kecil (akun baru, 6 transkripsi), unit pada setiap angka.
 */

import type { Segment, Transcript, TranscriptStatus } from '../api/transcripts';

export interface SampleTranscript extends Omit<Transcript, 'id'> {
	id: string;
}

const iso = (jamLalu: number) =>
	new Date(Date.now() - jamLalu * 3_600_000).toISOString();

const t = (
	i: number,
	title: string,
	status: TranscriptStatus,
	progress: number,
	duration_seconds: number | null,
	size_bytes: number,
	language: string | null,
	createdHour: number,
	error?: string,
	source_filename = '',
): SampleTranscript => ({
	id: `sample-${i}`,
	title,
	status,
	progress,
	source_filename: source_filename || `${title.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 24)}.m4a`,
	size_bytes,
	mime_type: 'audio/mp4',
	duration_seconds,
	language,
	requested_language: language ? null : 'id',
	model: 'whisper-large-v3-turbo',
	error: error ?? null,
	created_at: iso(createdHour),
	completed_at: status === 'completed' ? iso(createdHour - 0.3) : undefined
});

export const sampleTranscripts: SampleTranscript[] = [
	t(
		1,
		'Kuliah AI — Pertemuan 3',
		'completed',
		100,
		4_462.9, // 74m 23s
		38_924_672, // 37.1 MB
		'id',
		26
	),
	t(
		2,
		'Rapat sprint planning',
		'transcribing',
		42,
		null,
		21_682_176, // 20.7 MB
		null,
		1.2
	),
	t(3, 'Wawancara user #2', 'completed', 100, 1_942.0, 15_728_640, 'id', 	49, undefined, 'wawancara-user-2.wav'),
	t(4, 'Podcast sisi gelap data', 'completed', 100, 3_211.5, 29_360_128, 'en', 	71),
	t(5, 'Meeting mingguan tim', 'queued', 0, null, 8_388_608, null, 	0.2),
	t(
		6,
		'Diktat kuliah jaringan',
		'failed',
		0,
		null,
		52_428_800,
		null,
		8,
		'Layanan transkripsi sibuk (HTTP 429) — coba lagi nanti'
	)
];

// ===== Assertions: total harus cocok dengan baris (dibaca saat dev) =====
const completed = sampleTranscripts.filter((x) => x.status === 'completed');
const menitSelesai = completed.reduce((a, x) => a + (x.duration_seconds ?? 0), 0) / 60;
console.assert(completed.length === 3, '3 selesai (dipakai di copy deck)');
console.assert(menitSelesai > 0, 'total menit selesai dihitung dari baris');
export const sampleStats = {
	selesai: completed.length,
	berjalan: sampleTranscripts.filter((x) =>
		['queued', 'validating', 'chunking', 'transcribing', 'processing'].includes(x.status)
	).length,
	gagal: sampleTranscripts.filter((x) => x.status === 'failed').length,
	menitSelesai: Math.round(menitSelesai)
};

export const sampleDetail: {
	transcript: SampleTranscript;
	segments: Segment[];
	audio_url: string | null;
} = {
	transcript: {
		...sampleTranscripts[2],
		title: 'Wawancara user #2',
		duration_seconds: 1_942.0
	},
	segments: [
		seg(0, 0, 5_240, 'Terima kasih sudah mau meluangkan waktu. Sebelum mulai, bisa cerita sedikit pekerjaan Anda sehari-hari?'),
		seg(1, 5_240, 11_880, 'Jadi saya tiap hari buka tiga aplikasi berbeda untuk mencatat catatan rapat, dan itu melelahkan.'),
		seg(2, 11_880, 18_020, 'Kalau boleh tahu, bagian mana yang paling menyita waktu dari ketiganya?'),
		seg(3, 18_020, 26_400, 'Menyalin ulang audio. Saya harus memutar ulang rekaman dua sampai tiga kali untuk memastikan nama dan angkanya benar.'),
		seg(4, 26_400, 33_100, 'Dari skala satu sampai lima, seberapa sering Anda perlu membagikan hasil rapat itu ke orang lain?'),
		seg(5, 33_100, 39_640, 'Lima. Setiap rapat pasti ada yang tidak hadir, jadi notulennya wajib dikirim di hari yang sama.'),
		seg(6, 39_640, 47_280, 'Terakhir, kalau ada satu hal yang bisa hilang dari alur kerja Anda, apa itu?'),
		seg(7, 47_280, 55_620, 'Mungkin... proses memastikan siapa yang berkata apa. Dua pembicara kadang terdengar mirip di rekaman.')
	],
	audio_url: '/api/v1/transcripts/sample-3/audio'
};

function seg(idx: number, start_ms: number, end_ms: number, text: string): Segment {
	return { idx, start_ms, end_ms, text, confidence: 0.97, edited: idx === 3 };
}
