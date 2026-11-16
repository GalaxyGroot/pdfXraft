#!/usr/bin/env python3
"""PdfCraft speed benchmark (Python standard library only).

What it measures
  cli   pdfcraft-cli, headless: info, text, render, combine, split and edit, on a generated corpus.
  app   the desktop app, driven over its control channel (`--control`): launch to first frame,
        launch with a document to its first rendered page, opening a document in a running app,
        jumping to the last page and paging through, plus the app's memory.

Every app run gets its own empty settings folder (APPDATA / LOCALAPPDATA point at a temporary
directory), so the benchmark never reads or changes your real PdfCraft settings, and every run
starts from the same default state.

Usage (from the repository root)
  python bench/bench.py --app dist/pdfcraft.exe --cli target/release/pdfcraft-cli.exe
  python bench/bench.py --app dist/pdfcraft.exe --cli target/release/pdfcraft-cli.exe --quick
  python bench/bench.py --only cli --cli target/release/pdfcraft-cli.exe
  python bench/bench.py --compare bench/results/old.json bench/results/new.json

The corpus is generated on first use into bench/corpus/ (deterministic: the same files every time).
An app run opens windows on your screen for a few seconds each; leave the mouse and keyboard alone.
"""
import argparse
import ctypes
import json
import os
import platform
import random
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

# ---------------------------------------------------------------------------------------------
# Corpus: small deterministic PDFs written by hand (no third-party library).
# ---------------------------------------------------------------------------------------------


class Pdf:
    def __init__(self):
        self.objs = []

    def add(self, body):
        self.objs.append(body)
        return len(self.objs)

    def reserve(self):
        self.objs.append(None)
        return len(self.objs)

    def set(self, n, body):
        self.objs[n - 1] = body

    def stream(self, dictionary, data, compress=True):
        if compress:
            data = zlib.compress(data, 6)
            dictionary += " /Filter /FlateDecode"
        head = f"<< {dictionary} /Length {len(data)} >>\nstream\n".encode()
        return self.add(head + data + b"\nendstream")

    def save(self, path, root):
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for i, body in enumerate(self.objs, 1):
            offsets.append(len(out))
            out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
        start = len(out)
        out += f"xref\n0 {len(self.objs) + 1}\n0000000000 65535 f \n".encode()
        for off in offsets:
            out += f"{off:010d} 00000 n \n".encode()
        out += f"trailer\n<< /Size {len(self.objs) + 1} /Root {root} 0 R >>\nstartxref\n{start}\n%%EOF\n".encode()
        Path(path).write_bytes(bytes(out))


def build(path, pages):
    """`pages`: list of (content stream bytes, {xobject name: object number})."""
    pdf = Pdf()
    pages_ref = pdf.reserve()
    font = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    kids = []
    for content, xobjects in pages:
        contents = pdf.stream("", content)
        xo = " ".join(f"/{k} {v} 0 R" for k, v in xobjects.items())
        resources = f"<< /Font << /F1 {font} 0 R >> /XObject << {xo} >> >>"
        kids.append(
            pdf.add(f"<< /Type /Page /Parent {pages_ref} 0 R /MediaBox [0 0 612 792] /Resources {resources} /Contents {contents} 0 R >>".encode())
        )
    pdf.set(pages_ref, f"<< /Type /Pages /Kids [{' '.join(f'{k} 0 R' for k in kids)}] /Count {len(kids)} >>".encode())
    catalog = pdf.add(f"<< /Type /Catalog /Pages {pages_ref} 0 R >>".encode())
    pdf.save(path, catalog)


WORDS = (
    "the of and to in is that for it as was with be by on not he this are or his from at which but have an had they you were their one all "
    "we can her has there been if more when will would who so no she other its may these two like him into time has look more write go see "
    "number way could people my than first water been call who oil now find long down day did get come made may part"
).split()


def text_page(rng, lines=48, per_line=13):
    out = ["BT", "/F1 10 Tf", "12 TL", "40 760 Td"]
    for _ in range(lines):
        out.append("(" + " ".join(rng.choice(WORDS) for _ in range(per_line)) + ") Tj T*")
    out.append("ET")
    return "\n".join(out).encode()


def vector_page(rng, ops=4000):
    out = ["0.5 w"]
    for i in range(ops):
        x, y = rng.uniform(20, 590), rng.uniform(20, 770)
        if i % 5 == 0:
            out.append(f"{rng.random():.2f} {rng.random():.2f} {rng.random():.2f} rg {x:.1f} {y:.1f} {rng.uniform(4, 30):.1f} {rng.uniform(4, 30):.1f} re f")
        else:
            out.append(f"{x:.1f} {y:.1f} m {x + rng.uniform(-40, 40):.1f} {y + rng.uniform(-40, 40):.1f} {x + rng.uniform(-40, 40):.1f} {y + rng.uniform(-40, 40):.1f} {x + rng.uniform(-40, 40):.1f} {y + rng.uniform(-40, 40):.1f} c S")
    return "\n".join(out).encode()


def make_corpus(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    made = {}

    def want(name):
        path = directory / name
        made[name] = path
        return path if not path.exists() else None

    for n in (10, 100, 1000):
        path = want(f"text-{n}p.pdf")
        if path:
            rng = random.Random(1000 + n)
            build(path, [(text_page(rng), {}) for _ in range(n)])
    path = want("vector-30p.pdf")
    if path:
        rng = random.Random(7)
        build(path, [(vector_page(rng), {}) for _ in range(30)])
    path = want("image-30p.pdf")
    if path:
        # Six different 1600 x 2200 RGB images (smooth, so the file stays small; decoding them is the work).
        pdf = Pdf()
        pages_ref = pdf.reserve()
        font = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        width, height = 1600, 2200
        images = []
        for k in range(6):
            row = bytes(((x * (k + 2)) // 7 + k * 31) & 255 for x in range(width * 3))
            data = b"".join(row[(y * (k + 3)) % len(row):] + row[:(y * (k + 3)) % len(row)] for y in range(height))
            images.append(pdf.stream(f"/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8", data))
        kids = []
        rng = random.Random(11)
        for p in range(30):
            content = b"q 612 0 0 792 0 0 cm /Im0 Do Q\nq 0 0 0 rg BT /F1 14 Tf 40 40 Td (Page " + str(p + 1).encode() + b") Tj ET Q"
            contents = pdf.stream("", content)
            res = f"<< /Font << /F1 {font} 0 R >> /XObject << /Im0 {images[p % 6]} 0 R >> >>"
            kids.append(pdf.add(f"<< /Type /Page /Parent {pages_ref} 0 R /MediaBox [0 0 612 792] /Resources {res} /Contents {contents} 0 R >>".encode()))
        pdf.set(pages_ref, f"<< /Type /Pages /Kids [{' '.join(f'{k} 0 R' for k in kids)}] /Count {len(kids)} >>".encode())
        pdf.save(path, pdf.add(f"<< /Type /Catalog /Pages {pages_ref} 0 R >>".encode()))
    return made


# ---------------------------------------------------------------------------------------------
# Measuring helpers
# ---------------------------------------------------------------------------------------------


def summary(values):
    values = sorted(values)
    return {
        "n": len(values),
        "median": statistics.median(values),
        "min": values[0],
        "max": values[-1],
        "values": [round(v, 1) for v in values],
    }


def working_set_mb(pid):
    """(current, peak) working set of a process in MB, or None where it can't be read."""
    if platform.system() != "Windows":
        return None
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t), ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.restype = wintypes.HANDLE
    handle = kernel32.OpenProcess(0x0400 | 0x0010, False, pid)
    if not handle:
        return None
    try:
        counters = Counters()
        counters.cb = ctypes.sizeof(Counters)
        if not ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            return None
        return counters.WorkingSetSize / 1048576, counters.PeakWorkingSetSize / 1048576
    finally:
        kernel32.CloseHandle(handle)


class Control:
    """The app's control channel: newline-delimited JSON-RPC on a loopback port, after a token."""

    def __init__(self, control_file, process, timeout=60):
        self.file = Path(control_file)
        self.process = process
        deadline = time.perf_counter() + timeout
        self.sock = None
        while time.perf_counter() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"the app exited early (code {process.returncode})")
            try:
                info = json.loads(self.file.read_text(encoding="utf-8"))
                self.sock = socket.create_connection(("127.0.0.1", int(info["port"])), timeout=5)
                self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                break
            except (OSError, ValueError, KeyError):
                time.sleep(0.005)
        if self.sock is None:
            raise RuntimeError("the control channel never came up")
        self.reader = self.sock.makefile("r", encoding="utf-8", newline="\n")
        self.next_id = 1
        self.call("auth", token=info["token"])

    def call(self, method, **params):
        self.sock.sendall((json.dumps({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}) + "\n").encode())
        self.next_id += 1
        line = self.reader.readline()
        if not line:
            raise RuntimeError("the app closed the connection")
        reply = json.loads(line)
        if "error" in reply:
            raise RuntimeError(reply["error"].get("message", "error"))
        return reply.get("result")

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def page_ready(state):
    """A document is in front, its page has been drawn and nothing is waiting to be drawn."""
    active = (state or {}).get("active")
    if not active:
        return False
    raster = active.get("raster_memory") or {}
    return raster.get("page_count", 0) >= 1 and raster.get("pending_requests", 1) == 0 and bool(active.get("pages_on_screen"))


def wait_for(control, predicate, timeout=60, poll=0.004):
    deadline = time.perf_counter() + timeout
    while True:
        state = control.call("ui.state")
        if predicate(state):
            return state
        if time.perf_counter() > deadline:
            raise RuntimeError("timed out waiting for the app")
        time.sleep(poll)



def window_visible_ms(app, env_extra=None, timeout=20):
    """Launch WITHOUT the control channel and time until the app's window is on screen (Windows only).

    The control channel starts whoami/powershell/icacls to lock its token file, which adds about 2 s,
    so launch times taken through it are not what a user sees."""
    if platform.system() != "Windows":
        return None
    from ctypes import wintypes
    user32 = ctypes.WinDLL("user32")
    enum_proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    home = tempfile.mkdtemp(prefix="pdfcraft-bench-")
    env = dict(os.environ)
    env.update({"APPDATA": str(Path(home) / "Roaming"), "LOCALAPPDATA": str(Path(home) / "Local"), "USERPROFILE": home})
    env.update(env_extra or {})
    Path(env["APPDATA"]).mkdir(parents=True)
    Path(env["LOCALAPPDATA"]).mkdir(parents=True)
    t0 = time.perf_counter()
    process = subprocess.Popen([str(app)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=home)

    def shown(pid):
        found = []

        def each(hwnd, _):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid and user32.IsWindowVisible(hwnd):
                rect = wintypes.RECT()
                user32.GetWindowRect(hwnd, ctypes.byref(rect))
                if rect.right - rect.left > 300:
                    found.append(hwnd)
            return True

        user32.EnumWindows(enum_proc(each), 0)
        return bool(found)

    try:
        while time.perf_counter() - t0 < timeout:
            if shown(process.pid):
                return (time.perf_counter() - t0) * 1000
            time.sleep(0.005)
        return None
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        shutil.rmtree(home, ignore_errors=True)


class AppRun:
    """One launch of the app with its own empty settings folder."""

    def __init__(self, app, env_extra=None, file=None):
        self.home = tempfile.mkdtemp(prefix="pdfcraft-bench-")
        env = dict(os.environ)
        env["APPDATA"] = str(Path(self.home) / "Roaming")
        env["LOCALAPPDATA"] = str(Path(self.home) / "Local")
        env["USERPROFILE"] = self.home
        env.update(env_extra or {})
        Path(env["APPDATA"]).mkdir(parents=True)
        Path(env["LOCALAPPDATA"]).mkdir(parents=True)
        self.control_file = Path(self.home) / "control.json"
        args = [str(app), "--control", str(self.control_file)]
        if file:
            args.append(str(file))
        self.t0 = time.perf_counter()
        self.process = subprocess.Popen(args, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=self.home)
        self.control = None

    def connect(self):
        self.control = Control(self.control_file, self.process)
        return (time.perf_counter() - self.t0) * 1000

    def close(self):
        try:
            if self.control:
                self.control.close()
        finally:
            if self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.process.kill()
            shutil.rmtree(self.home, ignore_errors=True)


# ---------------------------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------------------------


def run_cli(cli, args, timeout=600):
    start = time.perf_counter()
    done = subprocess.run([str(cli), *map(str, args)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=timeout)
    ms = (time.perf_counter() - start) * 1000
    if done.returncode != 0:
        raise RuntimeError(f"{' '.join(map(str, args))}: exit {done.returncode}: {done.stderr.decode(errors='replace')[:200]}")
    return ms


def cli_scenarios(cli, corpus, work):
    c = corpus
    t100, t1000 = c["text-100p.pdf"], c["text-1000p.pdf"]
    out = work / "out.png"
    jobs = [("cli startup (info, 10 pages)", ["info", c["text-10p.pdf"]], 5)]
    for name, runs in (("text-100p.pdf", 5), ("text-1000p.pdf", 3), ("image-30p.pdf", 3), ("vector-30p.pdf", 3)):
        jobs.append((f"info {name}", ["info", c[name]], runs))
    for name, runs in (("text-100p.pdf", 5), ("text-1000p.pdf", 3)):
        jobs.append((f"text, all pages, {name}", ["text", c[name]], runs))
    jobs += [
        ("render page 1, 96 dpi, text-100p", ["render", t100, "--page", 1, "--dpi", 96, "--out", out], 5),
        ("render page 500, 96 dpi, text-1000p", ["render", t1000, "--page", 500, "--dpi", 96, "--out", out], 3),
        ("render page 1, 96 dpi, image-30p", ["render", c["image-30p.pdf"], "--page", 1, "--dpi", 96, "--out", out], 3),
        ("render page 1, 96 dpi, vector-30p", ["render", c["vector-30p.pdf"], "--page", 1, "--dpi", 96, "--out", out], 3),
        ("render page 1, 300 dpi, vector-30p", ["render", c["vector-30p.pdf"], "--page", 1, "--dpi", 300, "--out", out], 3),
        ("combine two 100-page files", ["combine", t100, t100, "--out", work / "combined.pdf"], 3),
        ("edit: rotate a page, 1000 pages", ["edit", t1000, "--rotate", "1:90", "--out", work / "edited.pdf"], 3),
        ("split 1000 pages every 100", ["split", t1000, "--every", 100, "--out-dir", work / "split"], 3),
    ]
    results = {}
    for title, args, runs in jobs:
        (work / "split").mkdir(exist_ok=True)
        try:
            run_cli(cli, args)  # warm-up (file cache)
            results[title] = summary([run_cli(cli, args) for _ in range(runs)])
            print(f"  {title:<46} {results[title]['median']:>9.0f} ms")
        except Exception as error:  # noqa: BLE001 - report and carry on
            results[title] = {"error": str(error)}
            print(f"  {title:<46} FAILED: {error}")
    return results


def app_scenarios(app, corpus, runs, env_variants):
    results = {}

    def record(title, values, extra=None):
        results[title] = summary(values)
        if extra:
            results[title].update(extra)
        print(f"  {title:<58} {results[title]['median']:>8.0f} ms")

    for label, env in env_variants:
        tag = f" [{label}]" if label != "default" else ""
        # 0. What a user sees: the window on screen, no control channel.
        shown = [v for v in (window_visible_ms(app, env) for _ in range(runs + 1)) if v][1:]  # first run warms the disk cache
        if shown:
            record(f"launch, no document: window on screen (no control channel){tag}", shown)
        # 1. Launch with nothing open, through the control channel (adds about 2 s of its own setup).
        ctl, first, mem = [], [], []
        for _ in range(runs):
            run = AppRun(app, env)
            try:
                ctl.append(run.connect())
                run.control.call("ui.state")
                first.append((time.perf_counter() - run.t0) * 1000)
                time.sleep(1.0)
                memory = working_set_mb(run.process.pid)
                if memory:
                    mem.append(memory[0])
            except Exception as error:  # noqa: BLE001
                print(f"  launch run failed: {error}")
            finally:
                run.close()
        if first:
            record(f"launch via control channel: first state answered{tag}", first, {"memory_mb_after_1s": round(statistics.median(mem), 1) if mem else None})
            record(f"launch via control channel: channel up{tag}", ctl)

        if label != "default" and env_variants[0][0] == "default":
            continue  # the variants only compare the launch

        for name in ("text-10p.pdf", "text-1000p.pdf", "image-30p.pdf", "vector-30p.pdf"):
            path = corpus[name]
            # 2. Launch with a document: until its first page is drawn.
            times, mem = [], []
            for _ in range(runs):
                run = AppRun(app, env, file=path)
                try:
                    run.connect()
                    wait_for(run.control, page_ready)
                    times.append((time.perf_counter() - run.t0) * 1000)
                    memory = working_set_mb(run.process.pid)
                    if memory:
                        mem.append(memory[1])
                except Exception as error:  # noqa: BLE001
                    print(f"  launch-with-document run failed: {error}")
                finally:
                    run.close()
            if times:
                record(f"launch with {name}: first page drawn (includes channel setup){tag}", times, {"peak_memory_mb": round(statistics.median(mem), 1) if mem else None})

            # 3. Open it in an app that is already running; then jump to the last page; then page forward.
            opens, jumps, pages = [], [], []
            for _ in range(runs):
                run = AppRun(app, env)
                try:
                    run.connect()
                    wait_for(run.control, lambda s: s is not None)
                    time.sleep(0.8)
                    start = time.perf_counter()
                    run.control.call("ui.open", path=str(path))
                    wait_for(run.control, page_ready)
                    opens.append((time.perf_counter() - start) * 1000)
                    if name in ("text-1000p.pdf", "image-30p.pdf"):
                        total = wait_for(run.control, page_ready)["active"].get("page_count") or None
                        start = time.perf_counter()
                        run.control.call("ui.type", text="G")
                        wait_for(run.control, lambda s: page_ready(s) and s["active"].get("page", 1) > 1)
                        jumps.append((time.perf_counter() - start) * 1000)
                        run.control.call("ui.type", text="g")
                        wait_for(run.control, lambda s: page_ready(s) and s["active"].get("page") == 1)
                        start = time.perf_counter()
                        for step in range(2, 12):
                            run.control.call("ui.type", text=".")
                            wait_for(run.control, lambda s, step=step: page_ready(s) and s["active"].get("page") == step)
                        pages.append((time.perf_counter() - start) * 1000)
                except Exception as error:  # noqa: BLE001
                    print(f"  open run failed: {error}")
                finally:
                    run.close()
            if opens:
                record(f"open {name} in the running app: first page drawn{tag}", opens)
            if jumps:
                record(f"  then jump to the last page (G){tag}", jumps)
            if pages:
                record(f"  then page forward ten times (.){tag}", pages)
    return results


# ---------------------------------------------------------------------------------------------


def compare(old_path, new_path):
    old, new = json.loads(Path(old_path).read_text()), json.loads(Path(new_path).read_text())
    print(f"{'scenario':<66} {'old ms':>9} {'new ms':>9} {'change':>8}")
    for section in ("cli", "app"):
        for title, now in (new.get(section) or {}).items():
            before = (old.get(section) or {}).get(title)
            if not before or "median" not in before or "median" not in now:
                continue
            change = (now["median"] - before["median"]) / before["median"] * 100 if before["median"] else 0
            mark = "  faster" if change < -5 else "  SLOWER" if change > 5 else ""
            print(f"{title:<66} {before['median']:>9.0f} {now['median']:>9.0f} {change:>+7.1f}%{mark}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--app", help="the pdfcraft executable to launch")
    parser.add_argument("--cli", help="the pdfcraft-cli executable")
    parser.add_argument("--only", choices=["cli", "app"], help="run only one group")
    parser.add_argument("--runs", type=int, default=5, help="runs per app scenario (default 5)")
    parser.add_argument("--quick", action="store_true", help="2 runs per app scenario")
    parser.add_argument("--variants", action="store_true", help="also time the launch with PDFCRAFT_RENDERER=gl and PDFCRAFT_SYSTEM_FONTS=0")
    parser.add_argument("--corpus", default=str(HERE / "corpus"), help="where the test PDFs are generated")
    parser.add_argument("--out", default=str(HERE / "results"), help="where the results are written")
    parser.add_argument("--label", default="", help="a name for this run, kept in the file name")
    parser.add_argument("--compare", nargs=2, metavar=("OLD", "NEW"), help="compare two result files and exit")
    args = parser.parse_args()

    if args.compare:
        compare(*args.compare)
        return 0
    if not args.app and not args.cli:
        parser.error("give --app and/or --cli (or --compare)")
    runs = 2 if args.quick else args.runs

    print("Generating the test PDFs (once)…")
    corpus = make_corpus(args.corpus)
    for name, path in corpus.items():
        print(f"  {name:<18} {path.stat().st_size / 1048576:7.1f} MB")

    report = {
        "when": time.strftime("%Y-%m-%d %H:%M:%S"),
        "machine": {"os": platform.platform(), "cpu": platform.processor(), "python": platform.python_version(), "cpus": os.cpu_count()},
        "label": args.label,
    }
    for key, path in (("app", args.app), ("cli", args.cli)):
        if path:
            exe = Path(path)
            report[f"{key}_file"] = {"path": str(exe), "mb": round(exe.stat().st_size / 1048576, 1), "modified": time.strftime("%Y-%m-%d %H:%M", time.localtime(exe.stat().st_mtime))}

    if args.cli and args.only != "app":
        print("\nHeadless (pdfcraft-cli):")
        with tempfile.TemporaryDirectory(prefix="pdfcraft-bench-cli-") as work:
            report["cli"] = cli_scenarios(Path(args.cli), corpus, Path(work))
    if args.app and args.only != "cli":
        print(f"\nDesktop app ({runs} runs each; windows will open and close):")
        variants = [("default", {})]
        if args.variants:
            variants += [("wgpu off, OpenGL", {"PDFCRAFT_RENDERER": "gl"}), ("installed fonts off", {"PDFCRAFT_SYSTEM_FONTS": "0"})]
        report["app"] = app_scenarios(Path(args.app), corpus, runs, variants)

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    name = time.strftime("%Y%m%d-%H%M%S") + (f"-{args.label}" if args.label else "") + ".json"
    (out / name).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"\nWritten: {out / name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
