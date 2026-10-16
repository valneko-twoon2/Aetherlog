#!/usr/bin/env python3
"""Repository language policy checks, used by the git hooks.

  check_english.py msg <commit-message-file>   commit message: English only, no Co-Authored-By
  check_english.py files <path>...             docs and source comments: English only

Product data is exempt: Japanese UI strings in code and templates, game data
under data/, and quoted UI labels in documentation (inline code, fenced code
and quotation marks) are allowed.  Everything else that is prose must be English.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

CJK = re.compile(r'[　-〿぀-ヿ㐀-䶿一-鿿＀-￯]')
TRAILER = re.compile(r'^\s*co-authored-by\s*:', re.IGNORECASE | re.MULTILINE)

SKIP_PARTS = {'data', 'icons', 'node_modules', 'target', 'dist', 'legacy_python', 'gen'}
# README.ja.md is the Japanese translation of README.md and is allowed to be Japanese.
SKIP_NAMES = {'package-lock.json', 'README.ja.md'}
DOC_SUFFIXES = {'.md'}
CODE_SUFFIXES = {'.rs', '.js', '.vue', '.css', '.py', '.sh', '.ps1', '.toml', '.html'}

# A comment marker followed, on the same line and before any quote, by CJK text.
COMMENT = re.compile(r'(?://|/\*|<!--|^\s*\*|^\s*#)[^"\'`]*' + CJK.pattern)


def check_message(path: Path) -> list[str]:
    lines = [ln for ln in path.read_text(encoding='utf-8', errors='replace').splitlines() if not ln.startswith('#')]
    text = '\n'.join(lines)
    problems = []
    if TRAILER.search(text):
        problems.append('commit message must not contain a Co-Authored-By trailer')
    if CJK.search(text):
        problems.append('commit message must be written in English')
    return problems


def strip_doc_exemptions(text: str) -> str:
    # The language switch link to the Japanese README is labelled in Japanese.
    text = re.sub(r'\[[^\]]*\]\(README\.ja\.md\)', '', text)
    text = re.sub(r'```.*?```', '', text, flags=re.DOTALL)
    text = re.sub(r'`[^`\n]*`', '', text)
    text = re.sub(r'「[^」\n]*」', '', text)
    return re.sub(r'"[^"\n]*"', '', text)


def check_file(path: Path) -> list[str]:
    if SKIP_PARTS & set(path.parts) or path.name in SKIP_NAMES:
        return []
    suffix = path.suffix.lower()
    if suffix not in DOC_SUFFIXES | CODE_SUFFIXES:
        return []
    text = path.read_text(encoding='utf-8', errors='replace')
    problems = []
    if suffix in DOC_SUFFIXES:
        for number, line in enumerate(strip_doc_exemptions(text).splitlines(), 1):
            if CJK.search(line):
                problems.append(f'{path}:{number}: documentation must be in English')
    else:
        for number, line in enumerate(text.splitlines(), 1):
            if COMMENT.search(line):
                problems.append(f'{path}:{number}: source comments must be in English')
    return problems


def main(argv: list[str]) -> int:
    if len(argv) < 3 or argv[1] not in {'msg', 'files'}:
        print(__doc__)
        return 2
    if argv[1] == 'msg':
        problems = check_message(Path(argv[2]))
    else:
        problems = [p for name in argv[2:] if Path(name).is_file() for p in check_file(Path(name))]
    for problem in problems:
        print(f'policy: {problem}', file=sys.stderr)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    sys.exit(main(sys.argv))
