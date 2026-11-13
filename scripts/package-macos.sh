#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd "$script_dir/.." && pwd)"
app_path="$project_root/target/release/bundle/osx/TauStash.app"
app_binary="$app_path/Contents/MacOS/taustash"
dmg_path="$project_root/target/release/bundle/dmg/TauStash.dmg"
mount_point=""
dmg_workspace=""

cleanup() {
    if [[ -n "$mount_point" ]]; then
        hdiutil detach "$mount_point" >/dev/null 2>&1 || true
    fi
    if [[ -n "$dmg_workspace" && -d "$dmg_workspace" ]]; then
        rm -rf "$dmg_workspace"
    fi
}
trap cleanup EXIT

for command in awk cargo codesign ditto du hdiutil ln mkdir mktemp rm sync tail; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "Required packaging command not found: $command" >&2
        exit 1
    }
done

test -f "$project_root/target/release/taustash"

cd "$project_root"
CARGO_BUNDLE_SKIP_BUILD=1 cargo bundle --release --format osx
test -f "$app_binary"

# Ad-hoc sign the complete bundle once every file is in place.
codesign --force --deep --sign - "$app_path"
codesign --verify --deep --strict --verbose=2 "$app_path"

dmg_workspace="$(mktemp -d "${TMPDIR:-/tmp}/taustash-dmg.XXXXXX")"
working_dmg="$dmg_workspace/TauStash-rw.dmg"
working_mount="$dmg_workspace/mount"
app_size_kb="$(du -sk "$app_path" | awk '{print $1}')"
image_size_kb=$((app_size_kb + app_size_kb / 5 + 32768))

mkdir -p "$working_mount"
hdiutil create \
    -size "${image_size_kb}k" \
    -fs HFS+ \
    -volname TauStash \
    "$working_dmg"

mount_point="$working_mount"
hdiutil attach \
    "$working_dmg" \
    -mountpoint "$mount_point" \
    -nobrowse \
    -noverify \
    -noautoopen
ditto "$app_path" "$mount_point/TauStash.app"
ln -s /Applications "$mount_point/Applications"
sync
hdiutil detach "$mount_point"
mount_point=""

mkdir -p "$(dirname "$dmg_path")"
rm -f "$dmg_path"
hdiutil convert \
    "$working_dmg" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -o "$dmg_path"

test -s "$dmg_path"
hdiutil verify "$dmg_path"

attach_output="$(hdiutil attach "$dmg_path" -readonly -nobrowse -noverify -noautoopen)"
mount_point="$(printf '%s\n' "$attach_output" | awk -F '\t' '$NF ~ /^\/Volumes\// { print $NF }' | tail -n 1)"
if [[ -z "$mount_point" ]]; then
    echo "Could not determine the mounted DMG path" >&2
    exit 1
fi

mounted_app="$mount_point/TauStash.app"
test -f "$mounted_app/Contents/MacOS/taustash"
test -f "$mounted_app/Contents/Resources/LICENSE"
test -L "$mount_point/Applications"
codesign --verify --deep --strict --verbose=2 "$mounted_app"

hdiutil detach "$mount_point"
mount_point=""

printf 'Created %s\n' "$dmg_path"
