#!/usr/bin/env bash
# Publish the workspace crates to crates.io in dependency order, skipping versions already
# published so a failed release can be re-run. Needs CARGO_REGISTRY_TOKEN.
set -euo pipefail
CRATES=(xr-video xr-core jellyfin-client)

index_path() {
  local n="$1"
  case "${#n}" in
    1|2) echo "${#n}/$n" ;;
    3) echo "3/${n:0:1}/$n" ;;
    *) echo "${n:0:2}/${n:2:2}/$n" ;;
  esac
}

metadata="$(cargo metadata --no-deps --format-version 1)"
for crate in "${CRATES[@]}"; do
  version="$(python3 -c 'import json,sys; print(next(p["version"] for p in json.loads(sys.argv[1])["packages"] if p["name"] == sys.argv[2]))' "$metadata" "$crate")"
  if curl -fsS "https://index.crates.io/$(index_path "$crate")" 2>/dev/null | grep -q "\"vers\":\"${version}\""; then
    echo "${crate} ${version} is already published; skipping"
    continue
  fi
  echo "Publishing ${crate} ${version}"
  cargo publish --locked -p "$crate"
done
