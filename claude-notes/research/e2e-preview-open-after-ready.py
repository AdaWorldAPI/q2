#!/usr/bin/env python3
"""End-to-end check that `q2 preview` opens the browser only AFTER the
server is accepting connections (bd-a6dvrdg1), via the real release
binary's own logs.

The `open` crate uses absolute /usr/bin/open on macOS, so PATH shimming
can't intercept it. Instead we observe the binary's tracing logs (-v):

  "Hub server listening (project mode)"     — emitted at TcpListener::bind
                                               (server.rs), the accept point
  "preview server accepting connections;
   opening browser"                          — emitted in the spawned task
                                               right before open::that, i.e.
                                               only after wait_until_accepting
                                               returned true

The fix is verified end-to-end iff the "opening browser" line appears
AFTER the "Hub server listening" line. We also independently poll the
port from this harness to timestamp readiness. A browser tab WILL open
— that is the live success demo.
"""
import os, re, socket, subprocess, sys, tempfile, time

BIN = os.path.abspath("target/release/q2")
URL_RE = re.compile(r"http://([0-9.]+):(\d+)")
TS_RE = re.compile(r"(\d{4}-\d{2}-\d{2}T[\d:.]+Z)")


def main():
    project = sys.argv[1] if len(sys.argv) > 1 else "docs"
    # The fmt layer writes to stdout; merge both streams so URL + logs
    # all land in one file we can poll.
    err = tempfile.NamedTemporaryFile(prefix="q2-preview-out-", suffix=".log",
                                      delete=False, mode="w+")
    # -v only enables quarto_hub; enable the binary crate's own target too.
    env = dict(os.environ, RUST_LOG="q2=info,quarto_hub=info")
    proc = subprocess.Popen(
        [BIN, "preview", project],
        stdout=err, stderr=subprocess.STDOUT, text=True, bufsize=1, env=env,
    )
    host = port = None
    try:
        # Poll the file for the printed URL.
        deadline = time.time() + 30
        while time.time() < deadline and proc.poll() is None:
            err.flush()
            with open(err.name) as f:
                for ln in f:
                    m = URL_RE.search(ln)
                    if m:
                        host, port = m.group(1), int(m.group(2))
                        break
            if host:
                break
            time.sleep(0.02)
        if host is None:
            print("FAIL: binary never printed a URL"); return 1

        # Independent readiness timestamp.
        t_accept = None
        deadline = time.time() + 30
        while time.time() < deadline:
            try:
                with socket.create_connection((host, port), timeout=0.2):
                    t_accept = time.time(); break
            except OSError:
                time.sleep(0.002)
        if t_accept is None:
            print("FAIL: port never accepted"); return 1

        # Wait until the binary logs that it is opening the browser.
        listening_line = opening_line = None
        deadline = time.time() + 8
        while time.time() < deadline:
            err.flush()
            with open(err.name) as f:
                lines = f.readlines()
            for ln in lines:
                if "Hub server listening" in ln and listening_line is None:
                    listening_line = ln.rstrip()
                if "opening browser" in ln and opening_line is None:
                    opening_line = ln.rstrip()
            if listening_line and opening_line:
                break
            time.sleep(0.05)

        print(f"project : {project}")
        print(f"port    : {port}")
        print(f"external poll: port first accepted a connection (T_accept observed)")
        print()
        print("relevant binary log lines (stderr, -v):")
        print(f"  [listening] {listening_line}")
        print(f"  [open]      {opening_line}")
        print()

        if not listening_line:
            print("FAIL: never saw 'Hub server listening'"); return 1
        if not opening_line:
            print("FAIL: never saw 'opening browser' — open was not reached"); return 1

        # Primary check: ordering within the binary's own log stream.
        with open(err.name) as f:
            body = f.read()
        i_listen = body.index("Hub server listening")
        i_open = body.index("opening browser")
        if i_open <= i_listen:
            print("FAIL: 'opening browser' appears BEFORE 'Hub server listening' "
                  "→ open not gated on readiness."); return 1

        # Secondary check: compare tracing timestamps if present.
        def ts(line):
            m = TS_RE.search(line or "")
            return m.group(1) if m else None
        t_listen, t_open = ts(listening_line), ts(opening_line)
        if t_listen and t_open:
            print(f"timestamps: listening={t_listen}  open={t_open}  "
                  f"(open >= listening: {t_open >= t_listen})")

        print()
        print("PASS: the real binary emitted 'opening browser' only AFTER "
              "'Hub server listening' (bind/accept). The readiness gate is "
              "wired through the real `q2 preview` path.")
        return 0
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        print(f"\n(full stderr captured at {err.name})")


if __name__ == "__main__":
    sys.exit(main())
