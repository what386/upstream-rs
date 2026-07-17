# Upstream managed PATH additions

let upstream_paths = [
    "/home/bmorin/Projects/programming/upstream-rs/tests/fakehome/.upstream/state/symlinks"
]

$env.PATH = ($upstream_paths ++ $env.PATH)
