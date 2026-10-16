"""C01 — Shared Inventory (reference solution)

Perbaikan:
- B1: `stok` dibuat sebagai instance attribute di __init__ sehingga tiap gudang punya dict sendiri.
- B2: `jumlah_stok` memakai dict.get(kode, 0) sesuai spesifikasi.
- B3: barang yang stoknya habis dihapus dari dict sehingga `jenis_barang` hanya menghitung stok > 0.
- B4: `daftar_barang` mengembalikan SALINAN dict, bukan objek internal.
- B5: `pindahkan` mengambil dari gudang asal lebih dulu; jika gagal, gudang tujuan tidak berubah.
"""


class Gudang:
    """Model satu gudang. Stok disimpan sebagai pemetaan kode barang -> jumlah unit."""

    satuan = "unit"   # class attribute: memang sama untuk semua gudang (immutable str)

    def __init__(self, nama):
        self.nama = nama
        self.stok = {}  # instance attribute: milik gudang ini saja

    def tambah(self, kode, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        self.stok[kode] = self.stok.get(kode, 0) + jumlah

    def ambil(self, kode, jumlah):
        tersedia = self.stok.get(kode, 0)
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        if jumlah > tersedia:
            raise ValueError("stok tidak mencukupi")
        sisa = tersedia - jumlah
        if sisa == 0:
            del self.stok[kode]
        else:
            self.stok[kode] = sisa

    def pindahkan(self, tujuan, kode, jumlah):
        self.ambil(kode, jumlah)      # validasi & pengurangan dulu
        tujuan.tambah(kode, jumlah)   # baru menambah ke tujuan

    def jumlah_stok(self, kode):
        return self.stok.get(kode, 0)

    def jenis_barang(self):
        return sum(1 for qty in self.stok.values() if qty > 0)

    def daftar_barang(self):
        return {kode: qty for kode, qty in self.stok.items() if qty > 0}

    def ringkasan(self):
        total = sum(self.stok.values())
        return f"{self.nama}: {self.jenis_barang()} jenis, {total} {self.satuan}"
