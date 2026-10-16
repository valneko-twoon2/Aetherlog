#!/usr/bin/env python3
"""Require a clean repository before an agent reports successful completion."""
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]


def dirty_files(root: Path) -> str:
    return subprocess.check_output(
        ['git', 'status', '--short', '--untracked-files=all'],
        cwd=root, encoding='utf-8', errors='replace', timeout=15,
    ).strip()


def decision(payload: dict, status: str) -> dict:
    if not status:
        return {}
    reason = (
        'The repository still has uncommitted changes. Review the diff, run the '
        'required checks, stage only changes authorized by the user, and commit '
        'them in focused English commits with the existing git hooks enabled. '
        'Do not use git add -A blindly, skip hooks, create an empty commit, '
        'discard changes, or push. Do not claim completion until the working '
        'tree is clean. If unrelated user changes or a failed check prevent '
        'completion, report the concrete blocker instead.\n' + status
    )
    if payload.get('stop_hook_active'):
        # Do not create an endless continuation loop when a real blocker remains.
        return {'continue': False, 'stopReason': 'Completion check failed. ' + reason}
    return {'decision': 'block', 'reason': reason}


def main() -> int:
    try:
        status = dirty_files(ROOT)
    except (OSError, subprocess.SubprocessError) as error:
        print(f'Cannot verify repository completion: {error}', file=sys.stderr)
        return 2
    if '--check' in sys.argv:
        if status:
            print(status)
            return 1
        print('Completion check passed: working tree is clean.')
        return 0
    try:
        payload = json.load(sys.stdin)
        if not isinstance(payload, dict):
            raise ValueError('Hook input must be an object')
    except ValueError as error:
        print(f'Invalid completion hook input: {error}', file=sys.stderr)
        return 2
    print(json.dumps(decision(payload, status)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
