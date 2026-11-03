#!/usr/bin/env bash
# migrate-herdr-projects.sh — move the agents an old herdr server runs (0.8.0 on
# the Mac mini) into new workspaces on the current herdr (0.9.5): one
# workspace per project, one tab per agent, each agent resumed in its tab.
#
#   scripts/migrate-herdr-projects.sh export [MANIFEST]   read the old server (read-only)
#   scripts/migrate-herdr-projects.sh plan   [MANIFEST]   print what import would do
#   scripts/migrate-herdr-projects.sh import [MANIFEST] [--no-start] [--with-shells] [--only PROJECT]...
#
# Data goes under the gitignored out/herdr-migrate/: the old server's raw
# lists (old-*.json) and the manifest (projects.json, the default MANIFEST).
#
# The usual order:
#   1. export, while the old server still runs;
#   2. stop the old server (this exits every pane process, agents included):
#        HERDR_SOCKET_PATH=~/.config/herdr/herdr.sock ~/herdr-versions/0.8.0/herdr server stop
#   3. start the current herdr (`herdr`, or `herdr server` headless);
#   4. import, from a terminal outside herdr or from any pane of the new one.
# Importing while the old agents still run would resume the same sessions
# twice; import refuses unless the old sessions' processes are gone
# (override with FORCE=1).
#
# Environment:
#   OLD_HERDR     old client         (default ~/herdr-versions/0.8.0/herdr)
#   OLD_SOCKET    old server socket  (default ~/.config/herdr/herdr.sock)
#   NEW_HERDR     current client     (default herdr on PATH)
#   NEW_SOCKET    current socket     (default: herdr's own; HERDR_SOCKET_PATH)
#   CLAUDE_ARGS   extra args for claude, e.g. "--permission-mode auto"
#   AGY_ARGS      extra args for agy
#   START_TIMEOUT ms to wait for each agent to come up (default 60000)
#   FORCE=1       import even if a session still looks live
#
# Resume commands: claude --resume <id> (in the session's original folder,
# where claude keeps it); agy --conversation <id>. Other kinds start fresh.

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DATA=$ROOT/out/herdr-migrate
OLD_HERDR=${OLD_HERDR:-$HOME/herdr-versions/0.8.0/herdr}
OLD_SOCKET=${OLD_SOCKET:-$HOME/.config/herdr/herdr.sock}
NEW_HERDR=${NEW_HERDR:-herdr}
START_TIMEOUT=${START_TIMEOUT:-60000}

die() { echo "migrate-herdr-projects: $*" >&2; exit 1; }

old() { env -u HERDR_PANE_ID -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
    HERDR_SOCKET_PATH="$OLD_SOCKET" "$OLD_HERDR" "$@"; }
new() {
    if [[ -n ${NEW_SOCKET:-} ]]; then
        HERDR_SOCKET_PATH="$NEW_SOCKET" "$NEW_HERDR" "$@"
    else
        "$NEW_HERDR" "$@"
    fi
}

cmd=${1:-}; shift || true
manifest=$DATA/projects.json
if [[ ${1:-} != "" && ${1:-} != --* ]]; then manifest=$1; shift; fi

# ---------------------------------------------------------------- export --
do_export() {
    [[ -x $OLD_HERDR ]] || die "no old herdr at $OLD_HERDR (set OLD_HERDR)"
    [[ -S $OLD_SOCKET ]] || die "no old server socket at $OLD_SOCKET (set OLD_SOCKET)"
    mkdir -p "$DATA" "$(dirname "$manifest")"
    old workspace list >"$DATA/old-workspaces.json"
    old tab list >"$DATA/old-tabs.json"
    old pane list >"$DATA/old-panes.json"
    old agent list >"$DATA/old-agents.json"
    python3 - "$DATA" "$manifest" <<'PY'
import json, os, re, subprocess, sys
data, out = sys.argv[1], sys.argv[2]
def load(name, key):
    d = json.load(open(os.path.join(data, "old-" + name)))
    if "error" in d:
        sys.exit(f"migrate-herdr-projects: old server: {d['error'].get('message')}")
    return d["result"][key]
workspaces = {w["workspace_id"]: w for w in load("workspaces.json", "workspaces")}
tabs = {t["tab_id"]: t for t in load("tabs.json", "tabs")}
panes = load("panes.json", "panes")
agents = {a["pane_id"]: a for a in load("agents.json", "agents")}

def claude_session_cwd(sid, pane_cwd):
    """The folder claude keeps the session under, where --resume finds it.

    claude stores a session in ~/.claude/projects/<cwd with every
    non-alphanumeric character as '-'>/<id>.jsonl. The folder is matched
    against the pane's cwd and the cwds the transcript records, since the
    first one recorded may be a folder since renamed.
    """
    base = os.path.expanduser("~/.claude/projects")
    try:
        dirs = os.listdir(base)
    except OSError:
        return None
    enc = lambda path: re.sub(r"[^A-Za-z0-9]", "-", path)
    for d in dirs:
        p = os.path.join(base, d, sid + ".jsonl")
        if not os.path.exists(p):
            continue
        seen = []
        with open(p, errors="replace") as f:
            for line in f:
                for m in re.finditer(r'"cwd":"((?:[^"\\]|\\.)*)"', line):
                    c = json.loads('"' + m.group(1) + '"')
                    if c not in seen:
                        seen.append(c)
        for c in [pane_cwd] + seen:
            if c and enc(c) == d and os.path.isdir(c):
                return c
        return next((c for c in reversed(seen) if os.path.isdir(c)), None)
    return None

def project_dir(path):
    """The main checkout of the repository holding path, else path itself."""
    if not path or not os.path.isdir(path):
        return path
    try:
        common = subprocess.run(
            ["git", "-C", path, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True, text=True, check=True).stdout.strip()
        return os.path.dirname(common) if common.endswith("/.git") else \
            subprocess.run(["git", "-C", path, "rev-parse", "--show-toplevel"],
                           capture_output=True, text=True, check=True).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return path

entries = []
for p in panes:
    a = agents.get(p["pane_id"])
    sess = (a or {}).get("agent_session") or {}
    kind = (a or {}).get("agent")
    sid = sess.get("value") if sess.get("kind") == "id" else None
    cwd = p.get("cwd") or p.get("foreground_cwd")
    if kind == "claude" and sid:
        cwd = claude_session_cwd(sid, cwd) or cwd
    title = ((a or {}).get("terminal_title_stripped") or "").strip()
    entries.append({
        "project": os.path.basename(project_dir(cwd) or "") or "home",
        "project_dir": project_dir(cwd),
        "cwd": cwd,
        "kind": kind,              # None for a plain shell pane
        "session_id": sid,
        "title": title,
        "old_pane": p["pane_id"],
        "old_tab_label": tabs.get(p["tab_id"], {}).get("label"),
        "old_workspace_label": workspaces.get(p["workspace_id"], {}).get("label"),
    })
json.dump({"source_socket": os.environ.get("OLD_SOCKET"), "entries": entries},
          open(out, "w"), indent=2)
n_agents = sum(1 for e in entries if e["kind"])
n_projects = len({e["project_dir"] for e in entries if e["kind"]})
print(f"wrote {out}: {n_agents} agents in {n_projects} projects, "
      f"{len(entries) - n_agents} plain shell panes")
PY
}

# ------------------------------------------------------- plan and import --
# Prints one line per action as TSV:
#   W <project> <project_dir>                  a new workspace
#   T <project> <cwd> <label> <name> <kind> <session_id>   a tab in it
make_plan() {
    WITH_SHELLS=$with_shells ONLY="${only[*]:-}" python3 - "$manifest" <<'PY'
import json, os, re, sys
m = json.load(open(sys.argv[1]))
with_shells = os.environ["WITH_SHELLS"] == "1"
only = set(os.environ["ONLY"].split()) if os.environ["ONLY"] else None
order, groups = [], {}
for e in m["entries"]:
    if not e["kind"] and not with_shells:
        continue
    if only and e["project"] not in only:
        continue
    key = e["project_dir"]
    if key not in groups:
        order.append(key)
        groups[key] = []
    groups[key].append(e)
used = set()
def name_for(project):
    base = re.sub(r"[^a-z0-9_-]+", "-", project.lower()).strip("-") or "agent"
    if not base[0].isalpha():
        base = "a" + base
    base = base[:28]
    n = 1
    while f"{base}-{n}" in used:
        n += 1
    used.add(f"{base}-{n}")
    return f"{base}-{n}"
for key in order:
    es = groups[key]
    project = es[0]["project"]
    print("\t".join(["W", project, key]))
    for e in es:
        # Drop a leading status glyph (claude's spinner) from the title.
        title = re.sub(r"^[^\w(\[]+", "", e["title"] or "").strip()
        label = (title or e["kind"] or "shell")[:40].replace("\t", " ").strip()
        name = name_for(project) if e["kind"] else "-"
        print("\t".join(["T", project, e["cwd"], label, name,
                         e["kind"] or "-", e["session_id"] or "-"]))
PY
}

agent_args() { # kind session_id → native args for a resumed agent, one per line
    local kind=$1 sid=$2
    case $kind in
        claude)
            [[ $sid != - ]] && printf '%s\n' --resume "$sid"
            # shellcheck disable=SC2086
            [[ -n ${CLAUDE_ARGS:-} ]] && printf '%s\n' $CLAUDE_ARGS ;;
        agy)
            [[ $sid != - ]] && printf '%s\n' --conversation "$sid"
            # shellcheck disable=SC2086
            [[ -n ${AGY_ARGS:-} ]] && printf '%s\n' $AGY_ARGS ;;
    esac
    return 0
}

json_get() { # path into stdin JSON, e.g. result.workspace.workspace_id
    python3 -c '
import json, sys
d = json.load(sys.stdin)
if "error" in d:
    sys.exit("herdr: " + d["error"].get("message", str(d["error"])))
for k in sys.argv[1].split("."):
    d = d[k]
print(d)' "$1"
}

do_plan() {
    [[ -f $manifest ]] || die "no manifest at $manifest; run export first"
    local kind ws_dir
    while IFS=$'\t' read -r kind a b c d e f; do
        if [[ $kind == W ]]; then
            ws_dir=$b
            echo
            echo "workspace \"$a\"  (cwd $b)"
        else
            local args; args=$(agent_args "$e" "$f" | tr '\n' ' ')
            if [[ $e == - ]]; then
                echo "  tab \"$c\"  shell in $b"
            else
                echo "  tab \"$c\"  $e ${args}  (as $d, in $b)"
            fi
        fi
    done < <(make_plan)
}

session_live() { # kind session_id → 0 if a process still names the session
    [[ $2 == - ]] && return 1
    pgrep -f -- "$2" >/dev/null 2>&1
}

do_import() {
    [[ -f $manifest ]] || die "no manifest at $manifest; run export first"
    new workspace list >/dev/null 2>&1 || {
        new workspace list >&2 || true
        die "the current herdr server is not reachable or not compatible; start it first"
    }
    local plan; plan=$(make_plan)
    if [[ ${FORCE:-0} != 1 && $no_start != 1 ]]; then
        local live=()
        while IFS=$'\t' read -r kind a b c d e f; do
            [[ $kind == T && $e != - ]] && session_live "$e" "$f" && live+=("$f ($a)")
        done <<<"$plan"
        if ((${#live[@]})); then
            printf '  %s\n' "${live[@]}" >&2
            die "these sessions still have a live process; stop the old server first, or FORCE=1"
        fi
    fi
    local ws="" ws_dir="" first_pane="" first_tab="" resp tab pane out
    while IFS=$'\t' read -r kind a b c d e f; do
        if [[ $kind == W ]]; then
            resp=$(new workspace create --cwd "$b" --label "$a" --no-focus)
            ws=$(json_get result.workspace.workspace_id <<<"$resp")
            first_pane=$(json_get result.root_pane.pane_id <<<"$resp")
            first_tab=$(json_get result.tab.tab_id <<<"$resp")
            ws_dir=$b
            echo "workspace $ws \"$a\""
            continue
        fi
        if [[ -n $first_pane ]]; then
            # The workspace's first tab takes the first agent.
            pane=$first_pane; tab=$first_tab; first_pane=""
            new tab rename "$tab" "$c" >/dev/null
            if [[ $b != "$ws_dir" ]]; then
                new pane run "$pane" "cd $(printf '%q' "$b")" >/dev/null
                sleep 0.5
            fi
        else
            resp=$(new tab create --workspace "$ws" --cwd "$b" --label "$c" --no-focus)
            tab=$(json_get result.tab.tab_id <<<"$resp")
            pane=$(json_get result.root_pane.pane_id <<<"$resp")
        fi
        if [[ $e == - || $no_start == 1 ]]; then
            echo "  tab $tab \"$c\""
            continue
        fi
        local args=()
        while IFS= read -r x; do [[ -n $x ]] && args+=("$x"); done < <(agent_args "$e" "$f")
        if out=$(new agent start "$d" --kind "$e" --pane "$pane" --timeout "$START_TIMEOUT" \
                ${args[@]+-- "${args[@]}"} 2>&1); then
            echo "  tab $tab \"$c\": $e started as $d"
        else
            echo "  tab $tab \"$c\": $e as $d did not report ready: $out" >&2
        fi
    done <<<"$plan"
}

# ------------------------------------------------------------------ main --
no_start=0; with_shells=0; only=()
while (($#)); do
    case $1 in
        --no-start) no_start=1 ;;
        --with-shells) with_shells=1 ;;
        --only) only+=("$2"); shift ;;
        *) die "unknown option: $1" ;;
    esac
    shift
done

case $cmd in
    export) do_export ;;
    plan) do_plan ;;
    import) do_import ;;
    *) awk 'NR > 1 && !/^#/ { exit } NR > 1 { sub(/^# ?/, ""); print }' "$0"; exit 2 ;;
esac
