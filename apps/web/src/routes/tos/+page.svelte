<script lang="ts">
	const tanggal = '31 Oktober 2026';
</script>

<svelte:head>
	<title>Syarat Layanan — TranscribeAI</title>
	<meta name="description" content="Ketentuan penggunaan layanan TranscribeAI." />
</svelte:head>

<div class="mx-auto max-w-3xl px-6 py-10">
	<header class="mb-8">
		<a href="/" class="flex items-center gap-2 font-semibold tracking-tight">
			<span class="text-accent" aria-hidden="true">◆</span>
			TranscribeAI
		</a>
		<h1 class="mt-6 text-3xl font-bold tracking-tight">Syarat Layanan</h1>
		<p class="mt-1 text-sm text-muted">Terakhir diperbarui: {tanggal}</p>
	</header>

	<div class="space-y-6 leading-relaxed text-pretty">
		<section>
			<h2 class="mb-2 font-semibold">1. Layanan</h2>
			<p class="text-sm text-muted">
				TranscribeAI menyediakan konversi audio menjadi teks (transkripsi) berbasis machine learning,
				dengan editor dan ekspor multi-format. Layanan disediakan "sebagaimana adanya" pada fase beta.
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">2. Akun</h2>
			<ul class="list-disc space-y-1 pl-5 text-sm text-muted">
				<li>Anda bertanggung jawab menjaga kerahasiaan kredensial akun Anda.</li>
				<li>Pendaftaran beta dapat memerlukan kode undangan.</li>
				<li>Kami dapat menonaktifkan akun yang menyalahgunakan layanan (rate limit, konten ilegal).</li>
			</ul>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">3. Penggunaan yang dilarang</h2>
			<ul class="list-disc space-y-1 pl-5 text-sm text-muted">
				<li>Mengunggah konten yang melanggar hukum atau hak pihak ketiga.</li>
				<li>Mentranskripsikan rekaman tanpa izin pihak yang direkam, bila diwajibkan hukum yang berlaku.</li>
				<li>Reverse engineering, scraping berlebihan, atau upaya mengganggu ketersediaan layanan.</li>
			</ul>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">4. Akurasi</h2>
			<p class="text-sm text-muted">
				Transkripsi otomatis tidak 100% akurat. Verifikasi manual tetap diperlukan untuk keperluan resmi,
				hukum, atau medis. Kami menyediakan editor untuk membantu koreksi.
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">5. Hak kekayaan</h2>
			<p class="text-sm text-muted">
				Anda mempertahankan kepemilikan penuh atas file audio dan hasil transkripsi Anda. Kami hanya
				memprosesnya sesuai kebijakan privasi.
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">6. Batasan tanggung jawab</h2>
			<p class="text-sm text-muted">
				Pada fase beta, layanan dapat berubah, mengalami gangguan, atau pembatasan kuota tanpa
				pemberitahuan sebelumnya. Tanggung jawab kami dibatasi sebesar biaya yang telah Anda bayarkan
				(saat ini: nol).
			</p>
		</section>

		<section>
			<h2 class="mb-2 font-semibold">7. Perubahan ketentuan</h2>
			<p class="text-sm text-muted">
				Perubahan ketentuan akan dipublikasikan di halaman ini dengan tanggal pembaruan.
			</p>
		</section>
	</div>

	<footer class="mt-10 border-t border-line pt-6 text-sm text-muted">
		<a href="/" class="hover:text-text">← Beranda</a> ·
		<a href="/privacy" class="hover:text-text">Kebijakan Privasi</a>
	</footer>
</div>
