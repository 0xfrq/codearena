"""C12 — The Missing Behavior (reference solution)

Perbaikan:
- B1: ekspor_semua memeriksa PERILAKU (bisa_diekspor), bukan hierarki class; objek tanpa export() ditolak TypeError.
- B2: Exportable diberi @runtime_checkable sehingga dapat dipakai dengan isinstance.
- B3: StrukKantin.export adalah method, bukan string data.
- B4: bisa_diekspor juga memastikan export dapat dipanggil (runtime_checkable hanya memeriksa keberadaan nama).
- B5: ekspor_semua menerima iterable apa pun (tuple, generator), bukan hanya list.
"""

from typing import Protocol, runtime_checkable


@runtime_checkable
class Exportable(Protocol):
    def export(self) -> str:
        ...


class LaporanBase:
    def export(self) -> str:
        raise NotImplementedError


class LaporanNilai(LaporanBase):
    def __init__(self, nim, nilai):
        self.nim = nim
        self.nilai = nilai

    def export(self) -> str:
        return f"NILAI;{self.nim};{self.nilai}"


class KartuPerpus:
    def __init__(self, nomor):
        self.nomor = nomor

    def export(self) -> str:
        return f"PERPUS;{self.nomor}"


class StrukKantin:
    def __init__(self, total):
        self.total = total

    def export(self) -> str:
        return f"KANTIN;{self.total}"


def bisa_diekspor(obj) -> bool:
    return isinstance(obj, Exportable) and callable(getattr(obj, "export", None))


def ekspor_semua(dokumen):
    hasil = []
    for d in dokumen:
        if not bisa_diekspor(d):
            raise TypeError(f"objek {type(d).__name__} tidak dapat diekspor")
        hasil.append(d.export())
    return hasil
