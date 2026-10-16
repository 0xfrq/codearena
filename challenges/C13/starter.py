"""C13 — The Broken E-Wallet (integrated capstone)

E-wallet kampus "DompetKu". Desain modular:
  1. data      : Transaksi (dataclass)            — bukti transaksi, harus immutable
  2. decorator : @catat(jenis)                     — mencatat transaksi SETELAH operasi berhasil
  3. akun      : Akun (abstrak) -> AkunMahasiswa, AkunMerchant
  4. pembayaran: DapatMembayar (Protocol), VoucherKampus, PemrosesPembayaran
"""

import functools
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Protocol, runtime_checkable


# ----------------------------------------------------------------- 1. data
@dataclass
class Transaksi:
    jenis: str
    jumlah: int


# ----------------------------------------------------------------- 2. decorator
def catat(jenis):
    """Decorator factory: menambahkan Transaksi(jenis, jumlah) ke riwayat akun
    hanya jika operasi berhasil."""

    def decorator(func):
        @functools.wraps(func)
        def wrapper(self, jumlah, *args, **kwargs):
            self._riwayat.append(Transaksi(jenis, jumlah))
            return func(self, jumlah, *args, **kwargs)

        return wrapper

    return decorator


# ----------------------------------------------------------------- 3. akun
class Akun(ABC):
    _riwayat = []

    def __init__(self, pemilik, saldo_awal=0):
        if saldo_awal < 0:
            raise ValueError("saldo awal tidak boleh negatif")
        self.pemilik = pemilik
        self.__saldo = saldo_awal

    @property
    def saldo(self):
        return self.__saldo

    @property
    def riwayat(self):
        """Salinan read-only (tuple) dari riwayat transaksi akun ini."""
        return self._riwayat

    def _ubah_saldo(self, delta):
        if self.__saldo + delta < 0:
            raise ValueError("saldo tidak cukup")
        self.__saldo += delta

    @abstractmethod
    def biaya_transfer(self, jumlah):
        """Biaya transfer keluar untuk jenis akun ini."""

    @catat("topup")
    def topup(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        self._ubah_saldo(jumlah)

    @catat("tarik")
    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        self._ubah_saldo(-jumlah)

    def bayar(self, jumlah):
        self.tarik(jumlah)
        return f"AKUN {self.pemilik} {jumlah}"


class AkunMahasiswa(Akun):
    BATAS_TARIK_HARIAN = 1_000_000

    def __init__(self, pemilik, saldo_awal=0):
        super().__init__(pemilik, saldo_awal)
        self.ditarik_hari_ini = 0

    def biaya_transfer(self, jumlah):
        return 0

    @catat("tarik")
    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        if self.ditarik_hari_ini + jumlah > self.BATAS_TARIK_HARIAN:
            raise ValueError("melebihi batas tarik harian")
        if jumlah > self.saldo:
            raise ValueError("saldo tidak cukup")
        self.__saldo = self.saldo - jumlah
        self.ditarik_hari_ini += jumlah


class AkunMerchant(Akun):
    def biaya_transfer(self, jumlah):
        return max(1_000, jumlah // 100)


# ----------------------------------------------------------------- 4. pembayaran
@runtime_checkable
class DapatMembayar(Protocol):
    def bayar(self, jumlah: int) -> str:
        ...


class VoucherKampus:
    """Voucher kantin dari BEM. Bukan Akun, tetapi DAPAT membayar."""

    def __init__(self, kode, nilai):
        self.kode = kode
        self.nilai = nilai

    def bayar(self, jumlah):
        if jumlah <= 0 or jumlah > self.nilai:
            raise ValueError("nilai voucher tidak cukup")
        self.nilai -= jumlah
        return f"VOUCHER {self.kode} {jumlah}"


class PemrosesPembayaran:
    def __init__(self):
        self.berhasil = []

    def proses(self, sumber, jumlah):
        """Menerima sumber dana APA PUN yang memiliki perilaku bayar(jumlah)."""
        if not isinstance(sumber, Akun):
            raise TypeError("sumber dana tidak didukung")
        hasil = sumber.bayar(jumlah)
        self.berhasil.append(hasil)
        return hasil

    def transfer(self, asal, tujuan, jumlah):
        biaya = asal.biaya_transfer(jumlah)
        asal.tarik(jumlah + biaya)
        tujuan.topup(jumlah)
        return biaya
