"""Exercise duration editing in a real Linux pseudoterminal; save the transcript."""
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import termios
import time
from pathlib import Path

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 150, 0, 0))
proc = subprocess.Popen(["target/debug/btm", "--average", "5m", "--rate", "250ms"],
                        stdin=slave, stdout=slave, stderr=slave,
                        env={**os.environ, "TERM": "xterm-256color"}, start_new_session=True)
os.close(slave)
transcript = bytearray()

def collect(seconds=1):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if select.select([master], [], [], min(0.1, end - time.monotonic()))[0]:
            try:
                transcript.extend(os.read(master, 65536))
            except OSError:
                break
    assert proc.poll() is None, "bottom exited during averaging smoke"

try:
    collect(5)
    print("Startup transcript:", repr(bytes(transcript[-8000:])))
    assert b"Average:" in transcript, "averaging indicator missing"
    for duration in [b"5s", b"34h", b"0"]:
        os.write(master, b"\x1b[19~")  # xterm F8
        collect()
        assert b"Enter: apply" in transcript, "duration editor missing"
        os.write(master, duration + b"\r")
        collect()
    os.write(master, b"q")
    assert proc.wait(timeout=5) == 0
finally:
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=5)
    Path("target/generated/tui-transcript.ansi").write_bytes(transcript)
    os.close(master)
print("PTY startup, F8 editor, 5s/34h/off, and clean quit passed")
