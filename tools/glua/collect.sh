#!/usr/bin/env bash
# Lists what each realm's VM implements / stubs and rebuilds the manifest + coverage page.
# An address stubbed in one realm but implemented in another (surface.* on the server) counts
# as implemented: a stub is only what no realm carries.
set -euo pipefail
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d)
for r in server client menu; do
    cargo run -q -p gm-script --example glua_list -- --realm $r | sort -u > "$tmp/p_$r"
    cargo run -q -p gm-script --example glua_list -- --realm $r --stubs | sort -u > "$tmp/s_$r"
    comm -23 "$tmp/p_$r" "$tmp/s_$r" >> "$tmp/impl_any"
done
sort -u "$tmp"/p_* > "$tmp/p_all"
sort -u "$tmp"/s_* > "$tmp/s_all"
sort -u -o "$tmp/impl_any" "$tmp/impl_any"
comm -23 "$tmp/s_all" "$tmp/impl_any" > "$tmp/s_final"
python3 tools/glua/build_manifest.py --present "$tmp/p_all" --stubs "$tmp/s_final"
