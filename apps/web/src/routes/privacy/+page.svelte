<script lang="ts">
	const tanggal = '31 Oktober 2026';
</script>

<svelte:head>
	<title>Kebijakan Privasi — TranscribeAI</title>
	<meta name="description" content="Bagaimana TranscribeAI mengumpulkan, menyimpan, dan melindungi data Anda." />
</svelte:head>

<div class="mx-auto max-w-3xl px-6 py-10">
	<header class="mb-8">
		<a href="/" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<h1 class="mt-6 text-3xl font-bold tracking-tight">Kebijakan Privasi</h1>
		<p class="mt-1 text-sm text-muted">Terakhir diperbarui: {tanggal}</p>
	</header>

	<div class="space-y-6 leading-relaxed text-pretty">
		<section>
			<h2 class="mb-2 font-semibold">1. Data yang kami kumpulkan</h2>
			<ul class="list-disc space-y-1 pl-5 text-sm text-muted">
				<li><strong>Akun:</strong> email, nama, dan hash password (Argon2id — password tidak pernah disimpan mentah).</li>
				<li><strong>File audio</strong> yang Anda unggah (sementara — lihat retensi).</li>
				<li><strong>Hasil transkripsi</strong> beserta timestamp per segment.</li>
				<li><strong>Log teknis:</strong> alamat IP, user agent, dan ID permintaan untuk keamanan & audit rate limit.</li>
			</ul>
			<p class="mt-2 text-sm text-muted">Kami tidak memasang tracker pihak ketiga dan tidak menjual data Anda.</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">2. Retensi & penghapusan</h2>
			<ul class="list-disc space-y-1 pl-5 text-sm text-muted">
				<li>File audio dihapus otomatis <strong>paling lama 30 hari</strong> setelah transkripsi selesai.</li>
				<li>Teks transkripsi tetap tersedia sampai Anda menghapusnya (soft delete + purge terjadwal).</li>
				<li>Anda dapat menghapus transcript kapan pun dari dashboard.</li>
			</ul>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">3. Pemrosesan oleh pihak ketiga</h2>
			<p class="text-sm text-muted">
				Transkripsi dilakukan oleh <strong>Groq, Inc.</strong> sebagai sub-processor (pemroses, bukan pengendali)
				melalui API mereka. File audio dikirim terenkripsi (TLS) hanya untuk keperluan transkripsi. Per
				dokumentasi Groq, data yang dikirim via API <strong>tidak digunakan untuk melatih model</strong> mereka.
				Selain Groq dan penyimpanan object storage kami, data tidak dibagikan ke pihak lain.
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">4. Keamanan</h2>
			<p class="text-sm text-muted">
				Password di-hash Argon2id; sesi memakai access token berumur pendek (15 menit) + refresh token
				terekam hanya sebagai hash SHA-256 dengan rotasi otomatis; audio & transkripsi terisolasi per
				pengguna di level query basis data.
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">5. Hak Anda</h2>
			<ul class="list-disc space-y-1 pl-5 text-sm text-muted">
				<li>Akses & ekspor data Anda (TXT/SRT/VTT/JSON) kapan pun.</li>
				<li>Penghapusan transcript secara mandiri.</li>
				<li>Penghapusan akun beserta seluruh datanya (hubungi kami — otomatisasi penuh menyusul).</li>
			</ul>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">6. Kontak</h2>
			<p class="text-sm text-muted">
				Pertanyaan soal privasi atau permintaan penghapusan data:
				<a class="text-accent hover:underline" href="mailto:privacy@transcribeai.example">privacy@transcribeai.example</a>
			</p>
		</section>
	</div>

	<footer class="mt-10 border-t border-line pt-6 text-sm text-muted">
		<a href="/" class="hover:text-text">← Beranda</a> ·
		<a href="/tos" class="hover:text-text">Syarat Layanan</a>
	</footer>
</div>
