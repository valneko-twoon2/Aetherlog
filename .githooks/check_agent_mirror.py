#!/usr/bin/env python3
"""Check that Claude and Codex share the same repository guidance."""
import subprocess
import sys


if __name__ == '__main__':
    # Inspect staged contents so unstaged edits cannot conceal a divergent commit.
    entries = subprocess.check_output(
        ['git', 'ls-files', '--stage', '-z', '--', '.claude', '.agents']
    ).split(b'\0')
    trees = {'.claude': {}, '.agents': {}}
    for entry in filter(None, entries):
        meta, path = entry.split(b'\t', 1)
        mode, blob, stage = meta.decode().split()
        tree, name = path.decode().split('/', 1)
        trees[tree][name] = (mode, blob, stage)
    left, right = trees.values()
    differences = sorted(name for name in left.keys() | right.keys()
                         if left.get(name) != right.get(name))
    if differences:
        print('Agent configuration mirror differs in the index:')
        print('\n'.join(differences))
        print('Run tools/sync_agents.ps1 with the edited source and stage both trees.')
        sys.exit(1)
