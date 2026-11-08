#!/usr/bin/env python3
"""PreToolUse guard for the QA agents (qa-tester, qa-refuter).

Two installation layers share this script (see the audit skill's safety.md):
  --scope agent     from the `hooks:` frontmatter of qa-*.md; always enforces.
  --scope settings  from .claude/settings.local.json for every tool call; enforces only when the
                    input carries an `agent_id` and an `agent_type` starting with `qa-`.
Exit 0 allows, exit 2 blocks (stderr is shown to the agent).

Policy for QA agents:
  - no Edit/Write/NotebookEdit;
  - Bash is an allowlist: app:control (not start/stop), the audit scripts (qa_fs.py, qa_session.py
    mark/maintenance/guard/wait, audit_state.py put/pending/models), sqlite3 -readonly, read-only
    text tools, read-only git; no command substitution, no variable assignments, no redirection
    except to /dev/null or between stdout/stderr, heredocs only for audit_state.py put;
  - no path reference into the dataset outside for-claude-tests, and none into the production or
    dev profile folders, in any tool input.
"""
from __future__ import annotations

import json
import os
import re
import shlex
import sys
from pathlib import Path

SCRIPTS = ".claude/skills/audit/scripts/"
READ_ONLY = {"cat", "head", "tail", "grep", "rg", "ls", "wc", "jq", "stat", "file", "identify",
             "ffprobe", "sort", "cut", "tr", "echo", "printf", "date", "pwd", "true", "basename",
             "dirname", "realpath", "diff", "cmp", "sha256sum", "md5sum", "column", "nl"}
# Options that make an otherwise read-only tool write files or run programs.
UNSAFE_OPTIONS = {
    "sort": re.compile(r"^(-o|--output|-[a-zA-Z]*o)"),
    "rg": re.compile(r"^(--pre|--pre-glob|-z|--search-zip)"),
    "date": re.compile(r"^(-s|--set)"),
    "identify": re.compile(r"^-(write|script)"),
    "ffprobe": re.compile(r"^-(o|report)"),
}
GIT_UNSAFE = re.compile(r"^(-O|--open-files-in-pager|--output|--ext-diff|--textconv|-c$|--exec)")
SCRIPT = re.compile(r"^(?:\./)?(?:/[^\s]+/)?\.claude/skills/audit/scripts/([a-z_]+\.py)$")
GIT_READ = {"log", "show", "diff", "blame", "status", "rev-parse", "ls-files", "grep"}
CONTROL_BLOCKED = {"start", "stop"}
QA_SESSION_ALLOWED = {"mark", "maintenance", "guard", "wait"}
AUDIT_STATE_ALLOWED = {"put", "pending", "models"}
SQLITE_BLOCKED = re.compile(r"\.(output|once|save|backup|import|shell|system|log|excel|restore|clone|open|load|cd|archive|ar|recover|expert)\b|"
                            r"\bvacuum\s+into\b|\battach\b|\bwritefile\b|\bedit\s*\(|\bload_extension\b|"
                            r"(^|\s)-(init|cmd)\b", re.I)
PROFILE = re.compile(r"com\.example\.mediatagger(\.dev)?(?![.\w-])")
FILE_TOOLS = {"Read", "Grep", "Glob", "LS"}
# Everything else is refused for QA agents (default deny), e.g. WebFetch, WebSearch, mcp__*.
OTHER_ALLOWED = {"StructuredOutput", "TodoWrite", "ToolSearch"}
WRITE_TOOLS = {"Edit", "Write", "NotebookEdit", "MultiEdit"}
OPERATORS = {"|", "||", "&&", ";", "&", "|&"}
REDIRECTS = {">", ">>", ">&", "&>", "&>>", "<", "<<", "<<<", ">|", "<>", "<&"}


def latest_run(cwd: str) -> Path | None:
    root = Path(os.environ.get("CLAUDE_PROJECT_DIR") or cwd or ".")
    try:
        latest = (root / "docs" / "review" / "LATEST").read_text(encoding="utf-8").strip()
    except OSError:
        return None
    if not re.fullmatch(r"[0-9A-Za-z._-]+", latest):
        return None
    run_dir = root / "docs" / "review" / latest
    return run_dir if (run_dir / "run.json").is_file() else None


def load_context(run_dir: Path | None) -> tuple[str | None, bool]:
    """(dataset realpath, whether a QA session is active) from the latest run, if any."""
    if run_dir is None:
        return None, False
    try:
        run = json.loads((run_dir / "run.json").read_text(encoding="utf-8"))
        return os.path.realpath(run["dataset"]), bool(run.get("active_session"))
    except (OSError, ValueError, KeyError):
        return None, False


def log_block(run_dir: Path | None, scope: str, payload: dict, reason: str) -> None:
    """Best-effort audit trail of every block; hook-check uses it to prove the hook fired."""
    if run_dir is None:
        return
    entry = {"scope": scope, "agent_type": payload.get("agent_type"), "tool": payload.get("tool_name"),
             "reason": reason, "input": json.dumps(payload.get("tool_input") or {}, ensure_ascii=False)[:400]}
    try:
        with open(run_dir / "hook-log.jsonl", "a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        pass


def dataset_violation(text: str, dataset: str | None) -> str | None:
    if PROFILE.search(text):
        return "references a production or dev profile folder"
    if not dataset:
        return None
    for match in re.finditer(re.escape(dataset), text):
        rest = text[match.end():]
        if not rest.startswith("/for-claude-tests") or rest.startswith("/for-claude-tests/.."):
            return "references the dataset outside for-claude-tests"
    if "/../" in text and "for-claude-tests" in text:
        return "uses .. next to for-claude-tests"
    return None


def split_segments(command: str) -> tuple[list[list[str]], str]:
    """Tokenize the first line (or the whole command) into operator-separated segments."""
    body = ""
    if "<<" in command:
        head, _, body = command.partition("\n")
    else:
        head = command
        if "\n" in command:
            raise ValueError("multi-line commands are only allowed for audit_state.py put heredocs")
    lexer = shlex.shlex(head, posix=True, punctuation_chars=";&|<>")
    lexer.whitespace_split = True
    lexer.commenters = ""
    tokens = list(lexer)
    segments: list[list[str]] = [[]]
    for token in tokens:
        if token in OPERATORS:
            segments.append([])
        else:
            segments[-1].append(token)
    return [segment for segment in segments if segment], body


def check_redirects(segment: list[str], allow_heredoc: bool) -> tuple[list[str], str | None]:
    words: list[str] = []
    index = 0
    while index < len(segment):
        token = segment[index]
        if token in REDIRECTS or re.fullmatch(r"\d?(>>?|>&|<)", token):
            target = segment[index + 1] if index + 1 < len(segment) else ""
            if token in ("<<",) and allow_heredoc:
                if not re.fullmatch(r"'?[A-Z_]+'?", target):
                    return words, "heredoc delimiters must be a quoted word"
            elif token in (">&", "<&") and target in ("1", "2"):
                pass
            elif token in (">", ">>", "&>") and target == "/dev/null":
                pass
            elif token == "<" and target and not target.startswith("-"):
                pass
            else:
                return words, f"redirection {token} {target} is not allowed"
            if words and re.fullmatch(r"\d", words[-1]):
                words.pop()
            index += 2
            continue
        words.append(token)
        index += 1
    return words, None


def check_segment(words: list[str]) -> str | None:
    if not words:
        return None
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", words[0]):
        return "environment assignments are not allowed"
    program = os.path.basename(words[0])
    args = words[1:]
    if program == "bun":
        if args[:2] != ["run", "app:control"]:
            return "only `bun run app:control ...` is allowed"
        if len(args) > 2 and args[2] in CONTROL_BLOCKED:
            return "sessions are started and stopped by qa_session.py through the clerk"
        return None
    if program in ("python3", "python"):
        flags = []
        while args and args[0].startswith("-"):
            flags.append(args.pop(0))
        if any(flag not in ("-I", "-u") for flag in flags) or not args:
            return "python3 may only run the audit scripts (flags -I/-u)"
        script = args[0]
        match = SCRIPT.match(script)
        if not match:
            return "python3 may only run the audit scripts"
        name = match.group(1)
        rest = [arg for arg in args[1:] if not arg.startswith("--run")]
        sub = next((arg for arg in rest if not arg.startswith("-") and not re.fullmatch(r"docs/review/\S+", arg)), "")
        if name == "qa_fs.py":
            return None
        if name == "qa_session.py":
            return None if sub in QA_SESSION_ALLOWED else f"qa_session.py {sub} is reserved for the clerk"
        if name == "audit_state.py":
            return None if sub in AUDIT_STATE_ALLOWED else f"audit_state.py {sub} is reserved for the coordinator"
        if name == "coverage.py":
            return None
        return f"{name} is not available to QA agents"
    if program == "sqlite3":
        if "-readonly" not in args:
            return "sqlite3 needs -readonly"
        if SQLITE_BLOCKED.search(" ".join(args)):
            return "sqlite3 dot-commands that write files or attach databases are blocked"
        return None
    if program == "git":
        if not args or args[0] not in GIT_READ:
            return "only read-only git subcommands are allowed"
        if any(GIT_UNSAFE.match(arg) for arg in args):
            return "git options that write files or run programs are blocked"
        return None
    if program == "find":
        if any(arg in ("-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprintf", "-fls") for arg in args):
            return "find may not delete, execute or write"
        return None
    if program == "sed":
        if any(arg.startswith("-i") or arg.startswith("--in-place") for arg in args) or "-n" not in args:
            return "sed is allowed only as `sed -n` without -i"
        return None
    if program in READ_ONLY:
        unsafe = UNSAFE_OPTIONS.get(program)
        if unsafe and any(unsafe.match(arg) for arg in args):
            return f"{program} option writes files or runs programs"
        return None
    return f"`{program}` is not on the QA allowlist (file changes go through qa_fs.py)"


def evaluate(payload: dict, scope: str, dataset: str | None, session_active: bool = False) -> tuple[bool, str]:
    if scope == "settings":
        agent_type = str(payload.get("agent_type") or "")
        if not payload.get("agent_id"):
            return True, "main session"
        if not agent_type and session_active and payload.get("tool_name") not in FILE_TOOLS:
            # While a QA session runs, a subagent of unknown type might be a QA agent: fail closed.
            return False, "subagent without agent_type; cannot tell whether it is a QA agent"
        if not agent_type.startswith("qa-"):
            return True, "not a QA agent"
    tool = payload.get("tool_name", "")
    tool_input = payload.get("tool_input") or {}
    if tool in WRITE_TOOLS:
        return False, f"{tool} is not available to QA agents"
    reason = dataset_violation(json.dumps(tool_input, ensure_ascii=False), dataset)
    if reason:
        return False, reason
    if tool in FILE_TOOLS or tool in OTHER_ALLOWED:
        return True, "read-only tool"
    if tool != "Bash":
        return False, f"{tool} is not available to QA agents"
    command = str(tool_input.get("command", ""))
    head = command.partition("\n")[0] if "<<" in command else command
    if "<<" in head and not re.search(r"<<-?\s*(['\"])[A-Z_]+\1", head):
        return False, "heredoc delimiters must be quoted (<<'EOF') so the body stays literal"
    if "$(" in head or "`" in head or "<(" in head or ">(" in head or "${" in head:
        return False, "command substitution is not allowed"
    if "MEDIATAGGER_QA_FS_INNER" in command:
        return False, "the qa_fs sandbox marker may not be set"
    try:
        segments, body = split_segments(command)
    except ValueError as error:
        return False, str(error)
    if body or "<<" in command:
        if not (len(segments) == 1 and "put" in segments[0] and segments[0].count("<<") == 1
                and any(SCRIPT.match(word) and word.endswith("audit_state.py") for word in segments[0])):
            return False, "heredocs are only allowed for a single audit_state.py put"
        delimiter = segments[0][segments[0].index("<<") + 1] if segments[0][-1] != "<<" else ""
        lines = body.rstrip("\n").split("\n")
        # Nothing may follow the closing delimiter, and it may not appear earlier.
        if not delimiter or lines[-1] != delimiter or delimiter in lines[:-1]:
            return False, "the heredoc must end with its delimiter and nothing after it"
    for segment in segments:
        words, problem = check_redirects(segment, allow_heredoc=bool(body))
        if problem:
            return False, problem
        problem = check_segment(words)
        if problem:
            return False, problem
    return True, "allowed"


def main(argv: list[str]) -> int:
    scope = "agent"
    if len(argv) >= 2 and argv[0] == "--scope" and argv[1] in ("agent", "settings"):
        scope = argv[1]
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        print("qa_hook: unreadable hook input; blocking", file=sys.stderr)
        return 2
    run_dir = latest_run(str(payload.get("cwd") or ""))
    dataset, session_active = load_context(run_dir)
    allowed, reason = evaluate(payload, scope, dataset, session_active)
    if allowed:
        return 0
    log_block(run_dir, scope, payload, reason)
    print(f"qa_hook blocked this call: {reason}. QA agents act only through app:control, qa_fs.py "
          "and the audit scripts (see .claude/skills/audit/references/safety.md).", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
