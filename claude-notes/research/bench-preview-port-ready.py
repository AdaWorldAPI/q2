#!/usr/bin/env python3
"""Measure the q2-preview race window: time between the URL being printed
(when the browser is opened today, preview.rs:145) and the port actually
accepting a TCP connection (axum::serve accept loop is live).

Spawns `q2 preview <project> --no-browser`, parses the printed
http://HOST:PORT URL, then busy-polls TcpStream-style connect() until the
first success. Reports the delta. Runs N trials, each on a fresh process.
"""
import re, socket, subprocess, sys, time

BIN = "target/release/q2"
URL_RE = re.compile(r"http://([0-9.]+):(\d+)")


def one_trial(project: str):
    proc = subprocess.Popen(
        [BIN, "preview", project, "--no-browser"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1,
    )
    host = port = None
    t_print = None
    try:
        # Read stdout lines until we see the URL. Stamp the moment we see it —
        # this is when open::that(url) fires today.
        for line in proc.stdout:
            m = URL_RE.search(line)
            if m:
                host, port = m.group(1), int(m.group(2))
                t_print = time.perf_counter()
                break
        if t_print is None:
            return None  # process died before printing

        # Poll connect() as fast as possible until the port accepts.
        deadline = t_print + 30.0
        attempts = 0
        while time.perf_counter() < deadline:
            attempts += 1
            try:
                with socket.create_connection((host, port), timeout=0.2):
                    t_accept = time.perf_counter()
                    return (t_accept - t_print) * 1000.0, attempts, port
            except OSError:
                time.sleep(0.002)  # 2ms between attempts; fine-grained
        return None
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


def main():
    project = sys.argv[1] if len(sys.argv) > 1 else "docs"
    trials = int(sys.argv[2]) if len(sys.argv) > 2 else 5
    print(f"project={project}  bin={BIN}  trials={trials}")
    results = []
    for i in range(trials):
        r = one_trial(project)
        if r is None:
            print(f"  trial {i+1}: FAILED (no URL or never accepted)")
            continue
        ms, attempts, port = r
        results.append(ms)
        print(f"  trial {i+1}: port={port}  ready after {ms:8.1f} ms  ({attempts} connect attempts)")
    if results:
        results.sort()
        n = len(results)
        print(f"\n  min={results[0]:.1f}ms  median={results[n//2]:.1f}ms  max={results[-1]:.1f}ms  (n={n})")


if __name__ == "__main__":
    main()
