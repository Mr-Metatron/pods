#!/bin/sh
# Give the nested Wayland compositor an actual guest XTest keyboard event.
set -eu

: "${DISPLAY:?Run the nested compositor under Xvfb}"
window=
for candidate in $(xdotool search --name '.*'); do
    name=$(xdotool getwindowname "$candidate")
    case "$name" in
        'Weston Compositor -'*) window=$candidate; break ;;
    esac
done
if [ -z "$window" ]; then
    echo "The nested Weston X11 window was not found." >&2
    echo "DISPLAY=$DISPLAY" >&2
    xdotool search --name '.*' getwindowname %@ >&2 || true
    exit 1
fi
xdotool windowmap --sync "$window"
xdotool windowfocus --sync "$window"
xdotool key --clearmodifiers --delay 50 Shift_L
