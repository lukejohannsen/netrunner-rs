#!/usr/bin/env bash
# Renders the desktop client's sound effects from the recipes in
# crates/netrunner_desktop/examples/render_sfx.rs and writes them, as Ogg
# Vorbis, into crates/netrunner_desktop/assets/sfx/.
#
#   scripts/render_sfx.sh            render and install into assets/sfx/
#   scripts/render_sfx.sh --play     render, install, then play each by name
#
# The WAVs land in target/sfx/wav/ either way. The encoder is whichever of
# ffmpeg, oggenc and VLC is installed, tried in that order; the client
# reads only .ogg (audio::is_recording_of), so with none of them there is
# nothing to install and the script says so.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wav="$root/target/sfx/wav"
out="$root/crates/netrunner_desktop/assets/sfx"

rm -rf "$wav"
cargo run --quiet --manifest-path "$root/Cargo.toml" -p netrunner_desktop --example render_sfx -- "$wav"

encode() {
    if command -v ffmpeg >/dev/null; then
        ffmpeg -loglevel error -y -i "$1" -c:a libvorbis -q:a 5 "$2"
    elif command -v oggenc >/dev/null; then
        oggenc --quiet -q 5 -o "$2" "$1"
    elif command -v cvlc >/dev/null; then
        cvlc -I dummy --quiet "$1" \
            --sout "#transcode{vcodec=none,acodec=vorb,ab=128,channels=1,samplerate=44100}:std{access=file,mux=ogg,dst=$2}" \
            vlc://quit 2>/dev/null
    else
        echo "No Ogg Vorbis encoder found: install ffmpeg, vorbis-tools or vlc." >&2
        exit 1
    fi
}

for file in "$wav"/*.wav; do
    encode "$file" "$out/$(basename "${file%.wav}").ogg"
done
echo "Wrote $(ls "$wav" | wc -l) sounds to ${out#"$root"/}"

if [[ "${1:-}" == "--play" ]]; then
    player="$(command -v pw-play || command -v paplay || command -v aplay)"
    for file in "$wav"/*.wav; do
        echo "$(basename "${file%.wav}")"
        "$player" "$file"
        sleep 0.4
    done
fi
