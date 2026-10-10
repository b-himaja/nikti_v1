use std::path::PathBuf;

use nikti_core::sidecar::{PythonSidecarStatus, SidecarSupervisor};

fn sidecar_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sidecar")
        .canonicalize()
        .expect("sidecar/ must exist at the repo root")
}

/// Demonstrates the sidecar skeleton actually works end to end: spawn,
/// Arrow IPC handshake, one heartbeat round trip, clean shutdown. Frozen
/// sidecar contract: transport is Arrow IPC (never JSON), lifecycle is
/// owned by exactly this supervisor.
#[test]
fn sidecar_handshake_heartbeat_and_clean_shutdown() {
    let mut supervisor = SidecarSupervisor::spawn("python3", &sidecar_dir())
        .expect("sidecar should spawn and complete the Arrow IPC handshake");

    match supervisor.status() {
        PythonSidecarStatus::Ready { python_env_hash } => {
            assert!(
                !python_env_hash.is_empty(),
                "INV-030: Ready must carry a non-empty environment hash"
            );
        }
        other => panic!("expected PythonSidecarStatus::Ready, got {other:?}"),
    }

    supervisor
        .heartbeat()
        .expect("heartbeat ping should receive a pong");

    let exit_status = supervisor
        .shutdown()
        .expect("shutdown should complete without killing the process");
    assert!(
        exit_status.success(),
        "sidecar must exit cleanly on shutdown, got {exit_status:?}"
    );
}
