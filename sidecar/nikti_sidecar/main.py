"""Nikti Python sidecar entrypoint.

Skeleton only (no Analysis procedures implemented yet): starts, performs an
Arrow IPC handshake over stdout, answers heartbeat pings over stdin with
pongs, and exits cleanly on a shutdown message or stdin EOF. See CLAUDE.md
Part 5 item 14 / Frozen sidecar contract — transport is Arrow IPC, never
JSON, and this process never opens a storage connection or constructs a
Report itself (INV-029, INV-015).
"""

import hashlib
import os
import sys
import time

import pyarrow as pa
import pyarrow.ipc as ipc

HANDSHAKE_SCHEMA = pa.schema(
    [
        ("msg_type", pa.string()),
        ("python_env_hash", pa.string()),
        ("pid", pa.int64()),
        ("timestamp_ms", pa.int64()),
    ]
)


def compute_python_env_hash() -> str:
    """INV-030: a hash of the pinned interpreter + dependency lockfile,
    computed once at sidecar startup. Placeholder until a real lockfile
    (uv.lock / poetry.lock) is wired up — a later task, not this pass."""
    payload = f"{sys.version}|pyarrow=={pa.__version__}".encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def make_message(msg_type: str, env_hash: str) -> pa.RecordBatch:
    return pa.record_batch(
        {
            "msg_type": [msg_type],
            "python_env_hash": [env_hash],
            "pid": [os.getpid()],
            "timestamp_ms": [int(time.time() * 1000)],
        },
        schema=HANDSHAKE_SCHEMA,
    )


def main() -> int:
    out = sys.stdout.buffer
    inp = sys.stdin.buffer
    env_hash = compute_python_env_hash()

    writer = ipc.new_stream(out, HANDSHAKE_SCHEMA)
    try:
        writer.write_batch(make_message("handshake", env_hash))
        out.flush()

        reader = ipc.open_stream(inp)
        while True:
            try:
                batch = reader.read_next_batch()
            except StopIteration:
                break

            msg_type = batch.column("msg_type")[0].as_py()
            if msg_type == "ping":
                writer.write_batch(make_message("pong", env_hash))
                out.flush()
            elif msg_type == "shutdown":
                break
    finally:
        writer.close()
        out.flush()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
