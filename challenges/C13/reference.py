"""C13 — The Broken E-Wallet (reference solution)

Perbaikan:
- B1 (materi 1): riwayat menjadi instance attribute (dibuat di __init__), bukan class attribute bersama.
- B2 (materi 2 & 4): AkunMahasiswa.tarik memakai _ubah_saldo milik parent, bukan self.__saldo
     (yang ter-mangling menjadi _AkunMahasiswa__saldo).
- B3 (materi 3): decorator @catat mencatat SETELAH operasi berhasil.
- B4 (materi 5): Transaksi frozen dan property riwayat mengembalikan tuple.
- B5 (materi 5): PemrosesPembayaran memeriksa perilaku (Protocol DapatMembayar), bukan class Akun.
"""

import functools
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Protocol, runtime_checkable


@dataclass(frozen=True)
class Transaksi:
    jenis: str
    jumlah: int


def catat(jenis):
    def decorator(func):
        @functools.wraps(func)
        def wrapper(self, jumlah, *args, **kwargs):
            hasil = func(self, jumlah, *args, **kwargs)
            self._riwayat.append(Transaksi(jenis, jumlah))
            return hasil

        return wrapper

    return decorator


class Akun(ABC):
    def __init__(self, pemilik, saldo_awal=0):
        if saldo_awal < 0:
            raise ValueError("saldo awal tidak boleh negatif")
        self.pemilik = pemilik
        self.__saldo = saldo_awal
        self._riwayat = []

    @property
    def saldo(self):
        return self.__saldo

    @property
    def riwayat(self):
        return tuple(self._riwayat)

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
        self._ubah_saldo(-jumlah)
        self.ditarik_hari_ini += jumlah


class AkunMerchant(Akun):
    def biaya_transfer(self, jumlah):
        return max(1_000, jumlah // 100)


@runtime_checkable
class DapatMembayar(Protocol):
    def bayar(self, jumlah: int) -> str:
        ...


class VoucherKampus:
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
        if not (isinstance(sumber, DapatMembayar) and callable(getattr(sumber, "bayar", None))):
            raise TypeError("sumber dana tidak didukung")
        hasil = sumber.bayar(jumlah)
        self.berhasil.append(hasil)
        return hasil

    def transfer(self, asal, tujuan, jumlah):
        biaya = asal.biaya_transfer(jumlah)
        asal.tarik(jumlah + biaya)
        tujuan.topup(jumlah)
        return biaya
