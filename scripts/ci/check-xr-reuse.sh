#!/usr/bin/env bash
# Compile and run an independent path consumer, then audit its resolved dependency graph.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${REPO_ROOT}/target}"
CONSUMER_DIR="$(mktemp -d)"
trap 'rm -rf "${CONSUMER_DIR}"' EXIT
export CONSUMER_DIR REPO_ROOT
python3 - <<'PY'
import json
import os
from pathlib import Path
root = Path(os.environ['REPO_ROOT'])
consumer = Path(os.environ['CONSUMER_DIR'])
(consumer / 'Cargo.toml').write_text('''[package]
name = "xr-reuse-smoke"
version = "0.1.0"
edition = "2021"
[workspace]
[dependencies]
xr-core = { path = %s }
xr-video = { path = %s }
''' % (json.dumps(str(root / 'crates/xr-core')), json.dumps(str(root / 'crates/xr-video'))))
(consumer / 'src').mkdir()
(consumer / 'src/main.rs').write_text('''use xr_core::{OpenXrContext, XrConfig};
use xr_video::{decoder::mock::MockHardwareDecoder, HardwareVideoDecoder, VideoCodec, StereoLayout, Eye};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut xr = OpenXrContext::new_simulated(&XrConfig::default());
    xr.poll_events()?;
    assert!(xr.is_session_running());
    let mut decoder = MockHardwareDecoder::new(3840, 1080, VideoCodec::Hevc);
    decoder.init(3840, 1080, VideoCodec::Hevc)?;
    assert_eq!(StereoLayout::SideBySide.eye_region(Eye::Right, 3840, 1080)?.x, 1920);
    let _ = xr_video::audio::MockAudioSink::new(Default::default());
    let _ = xr_core::EquirectLayerConfig::default();
    Ok(())
}
''')
PY
cp "${REPO_ROOT}/Cargo.lock" "${CONSUMER_DIR}/Cargo.lock"
CONSUMER_HOST="$(rustc -vV | sed -n 's/^host: //p')"
cargo metadata --offline --filter-platform "${CONSUMER_HOST}" --format-version 1 --manifest-path "${CONSUMER_DIR}/Cargo.toml" > "${CONSUMER_DIR}/metadata.json"
python3 - <<'PY'
import json
import os
from pathlib import Path
metadata = json.loads((Path(os.environ['CONSUMER_DIR']) / 'metadata.json').read_text())
for package in metadata['packages']:
    assert not package['name'].startswith(('linux-quest-', 'deskquest-')), package['name']
print('Independent XR consumer has no DeskQuest dependencies')
PY
cargo run --offline --locked --manifest-path "${CONSUMER_DIR}/Cargo.toml"
