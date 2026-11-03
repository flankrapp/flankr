#!/bin/sh
# Builds the flankr daemon and CLI in release and installs it as
# ~/.local/bin/flankr, on this machine or on others over SSH.
#
#   scripts/install.sh                      build, install here
#   scripts/install.sh --remote HOST ...    build, install on HOST (repeatable)
#   scripts/install.sh --local --remote HOST   both
#   scripts/install.sh --no-build ...       install the last release build
#   scripts/install.sh --restart ...        then restart each daemon onto it
#
# The binary is copied next to the old one and renamed over it, so a
# running daemon keeps its own copy until it is restarted. With --restart
# the script runs `~/.local/bin/flankr serve --restart` after installing,
# here and on each remote; without it, it never stops or starts one.
# A remote must be the same OS and CPU as this machine.
#
# Environment: CARGO_TARGET_DIR (default target-main), HERDR_SRC (default
# vendor/herdr).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target-main}
bin=$target/release/flankr
local_install='' build=1 remotes='' restart=''

while [ $# -gt 0 ]; do
    case $1 in
        --local) local_install=1 ;;
        --remote) [ $# -ge 2 ] || { echo "--remote needs a host" >&2; exit 2; }
                  remotes="$remotes $2"; shift ;;
        --no-build) build= ;;
        --restart) restart=1 ;;
        -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done
[ -n "$remotes" ] || local_install=1

if [ -n "$build" ]; then
    (cd "$root" && HERDR_SRC=${HERDR_SRC:-$root/vendor/herdr} \
        CARGO_TARGET_DIR=$target CARGO_INCREMENTAL=0 \
        cargo build --release -p flankr)
fi
[ -x "$bin" ] || { echo "no release build at $bin" >&2; exit 1; }
version=$("$bin" --version)

if [ -n "$local_install" ]; then
    mkdir -p "$HOME/.local/bin"
    cp "$bin" "$HOME/.local/bin/flankr.new"
    mv -f "$HOME/.local/bin/flankr.new" "$HOME/.local/bin/flankr"
    echo "installed $version as $HOME/.local/bin/flankr"
    if [ -n "$restart" ]; then
        "$HOME/.local/bin/flankr" serve --restart
    fi
fi

here=$(uname -sm)
for host in $remotes; do
    there=$(ssh -o BatchMode=yes -o ConnectTimeout=10 "$host" uname -sm)
    if [ "$there" != "$here" ]; then
        echo "$host: is $there, this build is for $here; skipped" >&2
        continue
    fi
    ssh -o BatchMode=yes "$host" 'mkdir -p ~/.local/bin && cat > ~/.local/bin/flankr.new && chmod 755 ~/.local/bin/flankr.new && mv -f ~/.local/bin/flankr.new ~/.local/bin/flankr' < "$bin"
    echo "$host: installed $(ssh -o BatchMode=yes "$host" '"$HOME"/.local/bin/flankr --version') as ~/.local/bin/flankr"
    if [ -n "$restart" ]; then
        # By its full path: a login shell over SSH may not have ~/.local/bin
        # on PATH.
        echo "$host: restarting the daemon"
        ssh -o BatchMode=yes "$host" '"$HOME"/.local/bin/flankr serve --restart'
    fi
done
