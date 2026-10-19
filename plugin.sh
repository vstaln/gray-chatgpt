#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/gray-chatgpt.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/gray-chatgpt" ;;
    *) rel="x86_64-apple-darwin/gray-chatgpt" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/gray-chatgpt" ;;
    *) rel="x86_64-unknown-linux-musl/gray-chatgpt" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/gray-chatgpt.exe" "$here/target/release/gray-chatgpt" "$here/target/debug/gray-chatgpt.exe" "$here/target/debug/gray-chatgpt"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "gray-chatgpt binary not found in $here" >&2
exit 1
