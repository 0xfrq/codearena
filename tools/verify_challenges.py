"""Verifikasi challenge bank Code Arena (dijalankan oleh instruktur / CI, BUKAN oleh server).

Untuk setiap challenge di folder challenges/:
  1. reference.py + test_regression.py  -> SEMUA test harus lulus.
  2. starter.py   + test_regression.py  -> setiap bug (B1, B2, ...) di meta.json harus
     memiliki minimal satu test `test_b<N>_...` yang GAGAL (bug benar-benar terdeteksi).
  3. meta.json harus lengkap dan setiap topic harus termasuk daftar topik resmi.

Kode yang dijalankan di sini hanyalah kode challenge milik platform sendiri (bukan submission
mahasiswa). Setiap varian dijalankan pada subprocess terpisah di direktori sementara.

Pemakaian:  python tools/verify_challenges.py [--json]
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHALLENGES = os.path.join(ROOT, "challenges")

REQUIRED_TOPICS = [
    # materi 1
    "konsep class", "object/instance", "struktur class", "attribute", "instance attribute",
    "class attribute", "method", "instance method", "membuat object", "akses attribute & method",
    "pemodelan objek",
    # materi 2
    "visibility property", "encapsulation", "public/protected/private", "name mangling",
    "pengendalian akses attribute", "validasi via property",
    # materi 3
    "class method", "static method", "decorator", "custom decorator", "chaining decorator",
    "*args & **kwargs",
    # materi 4
    "inheritance", "base class", "derived class", "overriding", "nested inheritance",
    "multiple inheritance",
    # materi 5
    "abstract method", "data class", "attribute mutability", "duck typing", "structural typing",
]

META_FIELDS = ["id", "title", "materi", "topics", "summary", "objectives", "spec", "bugs",
               "attack_procedure", "rubric_notes", "concept_map"]

RUNNER = r"""
import json, sys, unittest
sys.path.insert(0, '.')
suite = unittest.defaultTestLoader.loadTestsFromName('test_regression')
results = {}
class R(unittest.TestResult):
    def addSuccess(self, t): results[t.id().split('.')[-1]] = 'pass'
    def addFailure(self, t, e): results[t.id().split('.')[-1]] = 'fail'
    def addError(self, t, e): results[t.id().split('.')[-1]] = 'error'
suite.run(R())
print('@@RESULT@@' + json.dumps(results))
"""


def run_variant(cdir, source_name):
    tmp = tempfile.mkdtemp(prefix="ca_verify_")
    try:
        shutil.copy(os.path.join(cdir, source_name), os.path.join(tmp, "solution.py"))
        shutil.copy(os.path.join(cdir, "test_regression.py"), os.path.join(tmp, "test_regression.py"))
        proc = subprocess.run(
            [sys.executable, "-B", "-c", RUNNER],
            cwd=tmp, capture_output=True, text=True, timeout=60,
        )
        for line in proc.stdout.splitlines():
            if line.startswith("@@RESULT@@"):
                return json.loads(line[len("@@RESULT@@"):])
        return {"__import__": "error: " + (proc.stderr.strip().splitlines() or ["?"])[-1]}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def verify(cid):
    cdir = os.path.join(CHALLENGES, cid)
    problems = []
    with open(os.path.join(cdir, "meta.json"), encoding="utf-8") as f:
        meta = json.load(f)
    for field in META_FIELDS:
        if not meta.get(field):
            problems.append(f"meta.{field} kosong")
    for t in meta.get("topics", []):
        if t not in REQUIRED_TOPICS:
            problems.append(f"topic tidak dikenal: {t}")
    for b in meta.get("bugs", []):
        for k in ("id", "title", "location", "concept", "scenario", "attack", "fix"):
            if not b.get(k):
                problems.append(f"bug {b.get('id')} field {k} kosong")

    ref = run_variant(cdir, "reference.py")
    starter = run_variant(cdir, "starter.py")
    failing_ref = [n for n, r in ref.items() if r != "pass"]
    if failing_ref:
        problems.append(f"reference gagal: {failing_ref}")
    bug_detection = {}
    for b in meta.get("bugs", []):
        tag = "test_" + b["id"].lower() + "_"
        tests = [n for n in ref if n.startswith(tag)]
        caught = [n for n in tests if starter.get(n) != "pass"]
        bug_detection[b["id"]] = {"tests": len(tests), "caught_by": caught}
        if not tests:
            problems.append(f"{b['id']}: tidak ada regression test")
        elif not caught:
            problems.append(f"{b['id']}: starter tidak gagal pada test mana pun")
    return {
        "id": cid,
        "title": meta.get("title"),
        "topics": meta.get("topics", []),
        "reference_tests": len(ref),
        "reference_pass": len(ref) - len(failing_ref),
        "starter_fail": sum(1 for r in starter.values() if r != "pass"),
        "bugs": bug_detection,
        "problems": problems,
    }


def main():
    ids = sorted(d for d in os.listdir(CHALLENGES) if os.path.isdir(os.path.join(CHALLENGES, d)))
    reports = [verify(cid) for cid in ids]
    covered = {t for r in reports for t in r["topics"]}
    missing = [t for t in REQUIRED_TOPICS if t not in covered]
    ok = all(not r["problems"] for r in reports) and not missing and len(ids) >= 13
    if "--json" in sys.argv:
        print(json.dumps({"ok": ok, "challenges": reports, "missing_topics": missing}, indent=2))
    else:
        for r in reports:
            status = "OK " if not r["problems"] else "ERR"
            bugs = ", ".join(f"{b}:{len(v['caught_by'])}/{v['tests']}" for b, v in r["bugs"].items())
            print(f"[{status}] {r['id']} {r['title']:<32} ref {r['reference_pass']}/{r['reference_tests']} pass | "
                  f"starter {r['starter_fail']} fail | bugs {bugs}")
            for p in r["problems"]:
                print(f"      - {p}")
        print(f"\nchallenges: {len(ids)} | topics covered: {len(covered & set(REQUIRED_TOPICS))}/{len(REQUIRED_TOPICS)}")
        if missing:
            print("missing topics:", ", ".join(missing))
        print("RESULT:", "PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
