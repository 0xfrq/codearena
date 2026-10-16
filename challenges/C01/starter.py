"""C01 — Shared Inventory

Sistem inventaris gudang laboratorium kampus.
Setiap objek Gudang mewakili SATU gudang fisik dengan stok barangnya sendiri.
"""


class Gudang:
    """Model satu gudang. Stok disimpan sebagai pemetaan kode barang -> jumlah unit."""

    satuan = "unit"   # class attribute: satuan hitung yang sama untuk semua gudang
    stok = {}         # daftar stok gudang

    def __init__(self, nama):
        self.nama = nama

    def tambah(self, kode, jumlah):
        """Menambah stok barang `kode` sebanyak `jumlah` (harus > 0)."""
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        self.stok[kode] = self.stok.get(kode, 0) + jumlah

    def ambil(self, kode, jumlah):
        """Mengambil stok. Gagal (ValueError) jika stok tidak mencukupi."""
        tersedia = self.stok.get(kode, 0)
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        if jumlah > tersedia:
            raise ValueError("stok tidak mencukupi")
        self.stok[kode] = tersedia - jumlah

    def jumlah_stok(self, kode):
        """Jumlah unit untuk `kode`; barang yang tidak terdaftar bernilai 0."""
        return self.stok[kode]

    def jenis_barang(self):
        """Banyaknya jenis barang yang stoknya > 0."""
        return len(self.stok)

    def ringkasan(self):
        total = sum(self.stok.values())
        return f"{self.nama}: {self.jenis_barang()} jenis, {total} {self.satuan}"
