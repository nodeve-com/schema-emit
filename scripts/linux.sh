#!/bin/sh
# Run a command against this tree on a Linux host with Nix, for what only Linux runs (callgrind).
# rsync sends the tree as git sees it (.gitignore'd paths and .git stay behind); the host keeps its
# target dir between runs, so a rerun builds incrementally. The command runs with rustup (which
# reads rust-toolchain.toml), valgrind, and a C toolchain from nixpkgs.
#
#   scripts/linux.sh sh scripts/instructions.sh
#
# SCHEMA_EMIT_LINUX_HOST names the host (an ssh name), from the environment, else from .env.
set -e
cd "$(dirname "$0")/.."
if [ -z "$SCHEMA_EMIT_LINUX_HOST" ] && [ -f .env ]; then
  SCHEMA_EMIT_LINUX_HOST=$(sed -n 's/^SCHEMA_EMIT_LINUX_HOST=//p' .env)
fi
host=${SCHEMA_EMIT_LINUX_HOST:?set SCHEMA_EMIT_LINUX_HOST to an ssh host}
dir=.cache/schema-emit-linux
rsync -az --delete --filter=':- .gitignore' --exclude=.git --rsync-path="mkdir -p $dir/src && rsync" \
  ./ "$host:$dir/src/"
exec ssh "$host" "cd $dir/src && CARGO_TARGET_DIR=\$HOME/$dir/target \
  nix shell nixpkgs#rustup nixpkgs#valgrind nixpkgs#gcc -c $*"
