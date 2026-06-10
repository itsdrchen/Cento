#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICON_DIR="$ROOT_DIR/src-tauri/icons"
SVG="$ICON_DIR/cento.svg"
ICONSET="$ICON_DIR/cento.iconset"
ICNS="$ICON_DIR/cento.icns"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "rsvg-convert 未安装。请先运行：brew install librsvg" >&2
  exit 1
fi

if ! command -v iconutil >/dev/null 2>&1; then
  echo "iconutil 不可用。此脚本需要在 macOS 上运行。" >&2
  exit 1
fi

if [[ ! -f "$SVG" ]]; then
  echo "找不到 SVG：$SVG" >&2
  exit 1
fi

rm -rf "$ICONSET"
mkdir -p "$ICONSET"

render_png() {
  local size="$1"
  local output="$2"
  rsvg-convert -w "$size" -h "$size" "$SVG" -o "$output"
}

render_png 16 "$ICONSET/icon_16x16.png"
render_png 32 "$ICONSET/icon_16x16@2x.png"
render_png 32 "$ICONSET/icon_32x32.png"
render_png 64 "$ICONSET/icon_32x32@2x.png"
render_png 128 "$ICONSET/icon_128x128.png"
render_png 256 "$ICONSET/icon_128x128@2x.png"
render_png 256 "$ICONSET/icon_256x256.png"
render_png 512 "$ICONSET/icon_256x256@2x.png"
render_png 512 "$ICONSET/icon_512x512.png"
render_png 1024 "$ICONSET/icon_512x512@2x.png"

render_png 16 "$ICON_DIR/16x16.png"
render_png 32 "$ICON_DIR/32x32.png"
render_png 64 "$ICON_DIR/64x64.png"
render_png 128 "$ICON_DIR/128x128.png"
render_png 256 "$ICON_DIR/128x128@2x.png"
render_png 256 "$ICON_DIR/256x256.png"
render_png 512 "$ICON_DIR/256x256@2x.png"
render_png 512 "$ICON_DIR/512x512.png"
render_png 1024 "$ICON_DIR/512x512@2x.png"
render_png 1024 "$ICON_DIR/1024x1024.png"

iconutil --convert icns "$ICONSET" --output "$ICNS"
echo "Generated $ICNS"
