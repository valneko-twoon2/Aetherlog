#!/usr/bin/env python3
"""Claude Code PreToolUse hook for Bash: blocks commits that would bypass the repository policy.

Blocked: a Co-Authored-By trailer in the command, --no-verify (or -n) on git commit,
and changing core.hooksPath.  Exit code 2 tells Claude Code to refuse the call and
shows stderr to the model.
"""
import json
import re
import sys


def reason(command: str) -> str | None:
    if re.search(r'co-authored-by\s*:', command, re.IGNORECASE):
        return 'Commits in this repository must not carry a Co-Authored-By trailer (see AGENTS.md).'
    if re.search(r'git\b[^|;&\n]*\bcommit\b', command):
        if re.search(r'(^|\s)(--no-verify|-[a-zA-Z]*n[a-zA-Z]*)(\s|$)', command.split('commit', 1)[1]):
            return 'Do not skip the commit hooks (--no-verify); fix the reported problem instead.'
    if re.search(r'core\.hookspath', command, re.IGNORECASE) and 'git' in command:
        if not re.search(r'core\.hookspath\s+\.githooks\s*$', command.strip(), re.IGNORECASE):
            return 'core.hooksPath must stay at .githooks (see AGENTS.md).'
    return None


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        return 0
    command = (payload.get('tool_input') or {}).get('command', '')
    message = reason(command)
    if message:
        print(message, file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
