/** Format ringkas untuk UI — docs/07-ui-design.md. */

export function formatDuration(seconds: number | null | undefined): string {
	if (seconds == null || Number.isNaN(seconds)) return '—';
	const total = Math.round(seconds);
	const m = Math.floor(total / 60);
	const s = total % 60;
	if (m >= 60) {
		const h = Math.floor(m / 60);
		return `${h}j ${m % 60}m`;
	}
	return `${m}m ${String(s).padStart(2, '0')}s`;
}

export function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
	return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function formatRelative(iso: string): string {
	const then = new Date(iso).getTime();
	if (Number.isNaN(then)) return '—';
	const diff = Date.now() - then;
	const menit = Math.floor(diff / 60_000);
	if (menit < 1) return 'baru saja';
	if (menit < 60) return `${menit} menit lalu`;
	const jam = Math.floor(menit / 60);
	if (jam < 24) return `${jam} jam lalu`;
	const hari = Math.floor(jam / 24);
	if (hari < 30) return `${hari} hari lalu`;
	return new Date(iso).toLocaleDateString('id-ID', { day: 'numeric', month: 'short', year: 'numeric' });
}

export const STATUS_LABEL: Record<string, string> = {
	uploaded: 'Terkirim',
	queued: 'Menunggu',
	validating: 'Memeriksa',
	chunking: 'Memotong',
	transcribing: 'Mentranskripsi',
	processing: 'Memproses',
	completed: 'Selesai',
	failed: 'Gagal',
	cancelled: 'Dibatalkan'
};
