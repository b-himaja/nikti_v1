# ADR 0003: Sidecar transport — Arrow IPC stream over stdio pipes

## Status
Accepted

## Context
CLAUDE.md Part 3 freezes "Arrow IPC / Arrow Flight" as the Rust↔Python
transport and Part 5 item 14 forbids ever serializing table content to
JSON across this boundary, but does not choose between the two Arrow
options. INV-024 requires the sidecar to be a local child process, never
a remote/cloud service, which bears directly on this choice.

Two concrete mechanisms were considered:
- **Arrow Flight over a local loopback gRPC server** — the sidecar binds
  a gRPC server on `127.0.0.1`, Rust connects as a Flight client. Gives a
  standard RPC shape and would generalize more easily to a remote
  execution model, but that generalization is explicitly out of scope
  for v1 (INV-024, Part 2 "Reserved Extension Points"). It also requires
  port management, TLS/auth considerations even on loopback, and a gRPC
  stack on both sides for a v1 that never needs to leave the local
  machine.
- **Arrow IPC stream format over the child's stdin/stdout pipes** — no
  network stack, no port, no server lifecycle beyond the OS process
  itself. The supervisor already owns the child's stdio handles from
  `std::process::Command`, so this is the smallest mechanism that
  satisfies "local child process" literally rather than by convention.

## Decision
Use the Arrow IPC **streaming** format (not the random-access **file**
format, which requires a seekable source and pipes aren't seekable),
written and read directly over the spawned child's stdin/stdout. Two
independent unidirectional streams: Rust→Python on the child's stdin,
Python→Rust on the child's stdout, each starting with its own schema
message per the Arrow IPC stream framing. stderr is piped separately for
diagnostics and never mixed into the data stream.

## Consequences
- No gRPC/Flight dependency is needed anywhere in this codebase for v1.
- `SidecarSupervisor` (crates/nikti-core/src/sidecar/supervisor.rs) is a
  thin wrapper around `arrow::ipc::reader::StreamReader` /
  `arrow::ipc::writer::StreamWriter` directly on `ChildStdin`/`ChildStdout`
  — no intermediate framing protocol was invented.
- If remote/distributed execution is ever pursued, it is a genuinely new
  transport (Flight or otherwise) and a new ADR, not a natural extension
  of this one — this decision is deliberately scoped to the local,
  single-child-process case in INV-024.
- Verified end-to-end in this pass: `nikti_sidecar/main.py` performs a
  real handshake/heartbeat/shutdown over this transport, and
  `crates/nikti-core/tests/sidecar_supervisor.rs` exercises the Rust side
  against the real Python process (not mocked).
