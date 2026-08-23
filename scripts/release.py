#!/usr/bin/env python3
"""phi-agent GitHub Release helper.

Drives the phi-agent GitHub Release and — critically — VERIFIES the release
actually exists. CI green is not enough: a workflow run can succeed while the
release step fails silently, leaving a tag with no Release, or a Release with
missing assets. "Released" here means all of:

  * the tag-triggered release.yml workflow run concluded `success`
  * a GitHub Release exists for the tag
  * it is not a draft or prerelease
  * every expected binary asset is attached

Run from the phi-agent repo root (or anywhere — paths are resolved relative to
this file). Sibling dependency repos (agent-base, agent-works,
phi-kernel-tools, phi-tools) are located as `../<crate>` and preflighted —
latest CI green, no stray working-tree changes — before any tag is pushed,
because a release ships the whole chain, not just phi-agent.

Known local-dev allowance: a dirty Cargo.toml / Cargo.lock is expected (the
`path = "../<crate>"` overrides you must remove before committing). Anything
else dirty fails preflight.

Requires: git + gh on PATH, git remote `origin` reachable, Python 3.8+.
Stdlib only — no third-party deps.

Usage:
  python3 scripts/release.py --version 0.11.2             full flow (default)
  python3 scripts/release.py --version 0.11.2 --dry-run   show, don't execute
  python3 scripts/release.py --check                      preflight only
  python3 scripts/release.py --verify --version 0.11.1    re-verify an existing tag
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

# ── Constants ────────────────────────────────────────────────────────────────

# Bottom-up dependency chain, preflighted as a unit before any release.
CHAIN = ["agent-base", "agent-works", "phi-kernel-tools", "phi-tools", "phi-agent"]

# Known local-dev overrides: these two files may be dirty while developing.
# Keep them uncommitted; anything else dirty is a real problem.
DIRTY_ALLOWED = {"Cargo.toml", "Cargo.lock"}

# Binary assets phi-agent's release.yml matrix produces. Update these if the
# matrix in .github/workflows/release.yml changes (or override with --assets).
PHI_AGENT_ASSETS = [
    "phi-linux-x86_64.tar.gz",
    "phi-linux-arm64.tar.gz",
    "phi-darwin-x86_64.tar.gz",
    "phi-darwin-arm64.tar.gz",
]

RELEASE_TIMEOUT_MIN = 20  # phi-agent builds 4 targets; test-gate + builds ≈ 6 min


class ReleaseError(Exception):
    """A step failed; message is user-facing."""


@dataclass
class Repo:
    name: str            # directory / crate name, e.g. "phi-agent"
    path: Path           # absolute path
    owner_repo: str      # "hibuka-labs/phi-agent"
    default_branch: str  # "master"
    has_release_wf: bool


# ── Plumbing ─────────────────────────────────────────────────────────────────

def run(cmd, *, cwd=None, check=True):
    """Run a command, capturing output. Raises ReleaseError with context."""
    try:
        proc = subprocess.run(
            cmd, cwd=cwd, capture_output=True, text=True, timeout=120
        )
    except subprocess.TimeoutExpired:
        raise ReleaseError(f"timed out after 120s: {' '.join(cmd)}")
    if check and proc.returncode != 0:
        raise ReleaseError(
            f"command failed ({proc.returncode}): {' '.join(cmd)}\n"
            f"  stdout: {proc.stdout.strip()}\n  stderr: {proc.stderr.strip()}"
        )
    return proc


def git(repo: Repo, *args, check=True):
    return run(["git", "-C", str(repo.path), *args], check=check)


def gh(repo: Repo, *args, check=True):
    return run(["gh", "-R", repo.owner_repo, *args], check=check)


def gh_json(repo: Repo, *args):
    out = gh(repo, *args, "--jq", ".").stdout
    return json.loads(out)


def resolve_repo(name: str) -> Repo:
    """Locate a chain repo and read its remote + default branch."""
    root = Path(__file__).resolve().parent.parent
    path = root.parent / name if name != "phi-agent" else root
    if not path.is_dir():
        raise ReleaseError(f"repo not found: {path}")

    url = git(Repo(name, path, "", ""), "remote", "get-url", "origin").stdout.strip()
    m = re.search(r"(?:github\.com[:/])([\w.-]+/[\w.-]+?)(?:\.git)?$", url)
    if not m:
        raise ReleaseError(f"cannot parse owner/repo from origin url: {url!r}")
    owner_repo = m.group(1)

    default_branch = gh_json(Repo(name, path, owner_repo, ""),
                             "api", "repos", owner_repo, "--jq", ".default_branch")
    has_release_wf = (path / ".github/workflows/release.yml").exists()
    return Repo(name, path, owner_repo, default_branch, has_release_wf)


def verify_gh_cli():
    if subprocess.run(["gh", "--version"], capture_output=True).returncode != 0:
        raise ReleaseError("`gh` not found on PATH — install GitHub CLI first")


# ── Phase 1: preflight ───────────────────────────────────────────────────────

def check_working_tree(repo: Repo, verbose: bool) -> None:
    dirty = git(repo, "status", "--porcelain").stdout.splitlines()
    real = sorted(
        line[3:].split(" -> ")[-1]  # rename "A -> B" takes B
        for line in dirty
        if line[:2] not in ("??",)  # keep untracked out of the "overrides" allowance
    )
    untracked = sorted(line[3:] for line in dirty if line[:2] == "??")
    unexpected = [f for f in real if f not in DIRTY_ALLOWED] + untracked
    if unexpected:
        raise ReleaseError(
            f"{repo.name}: dirty working tree — unexpected files: {unexpected}"
        )
    overrides = [f for f in real if f in DIRTY_ALLOWED]
    if overrides and verbose:
        print(f"  {repo.name}: {overrides} dirty (expected local path overrides, OK)")


def check_ci(repo: Repo, verbose: bool) -> None:
    runs = gh_json(
        repo, "run", "list", "--branch", repo.default_branch,
        "--limit", "3", "--json",
        "status,conclusion,headSha,createdAt,displayTitle",
    )
    completed = [r for r in runs if r["status"] == "completed"]
    if not completed:
        raise ReleaseError(
            f"{repo.name}: no completed CI run on {repo.default_branch} "
            f"— cannot confirm green before releasing"
        )
    latest = completed[0]
    if latest["conclusion"] != "success":
        raise ReleaseError(
            f"{repo.name}: latest CI on {repo.default_branch} concluded "
            f"`{latest['conclusion']}` ({latest['displayTitle']}) — fix before releasing"
        )
    if verbose:
        for r in runs:
            mark = "✓" if r["conclusion"] == "success" else r["status"]
            print(f"  {repo.name}: {mark}  {r['headSha'][:8]}  "
                  f"{r['displayTitle'][:48]}")


def committed_version(repo: Repo) -> str:
    toml = git(repo, "show", "HEAD:Cargo.toml").stdout
    m = re.search(r'^version\s*=\s*"([^"]+)"', toml, re.M)
    if not m:
        raise ReleaseError(f"{repo.name}: no version field in HEAD:Cargo.toml")
    return m.group(1)


def check_version_and_changelog(repo: Repo, version: str, verbose: bool) -> None:
    actual = committed_version(repo)
    if actual != version:
        raise ReleaseError(
            f"{repo.name}: HEAD Cargo.toml version is {actual}, expected {version}"
        )
    changelog = repo.path / "CHANGELOG.md"
    if changelog.exists():
        text = changelog.read_text()
        if f"## [{version}]" not in text:
            raise ReleaseError(
                f"{repo.name}: CHANGELOG.md has no [{version}] entry"
            )
    elif verbose:
        print(f"  {repo.name}: no CHANGELOG.md (nothing to check)")


def preflight(version: str | None, verbose: bool) -> Repo:
    print("── preflight ──")
    repos = [resolve_repo(name) for name in CHAIN]
    for repo in repos:
        check_working_tree(repo, verbose)
        check_ci(repo, verbose)
    print("  chain: working trees clean (except allowed overrides), CI all green")
    target = resolve_repo(version and "phi-agent" or "phi-agent")
    if version:
        check_version_and_changelog(target, version, verbose)
        print(f"  {target.name}: HEAD version {version}, CHANGELOG entry present")
    print("  preflight OK\n")
    return target


# ── Phase 2: exec (tag + push) ───────────────────────────────────────────────

def exec_release(repo: Repo, version: str, push_commit: bool, dry_run: bool) -> str:
    print("── exec ──")
    tag = f"v{version}"

    if push_commit:
        remote_head = git(repo, "rev-parse", "HEAD").stdout.strip()
        remote_ref = git(repo, "rev-parse", f"origin/{repo.default_branch}").stdout.strip()
        if remote_head != remote_ref:
            cmd = ["git", "-C", str(repo.path), "push", "origin",
                   f"HEAD:{repo.default_branch}"]
            print(f"  push HEAD ({remote_head[:8]}) → origin/{repo.default_branch}: "
                  f"{' '.join(cmd)}")
            if not dry_run:
                run(cmd)
        else:
            print(f"  HEAD already on origin/{repo.default_branch}, nothing to push")

    existing = git(repo, "rev-parse", "-q", "--verify", tag, check=False).stdout.strip()
    if existing:
        if existing != git(repo, "rev-parse", "HEAD").stdout.strip():
            raise ReleaseError(
                f"{repo.name}: tag {tag} already exists at {existing[:8]}, "
                f"not at HEAD {git(repo, 'rev-parse', 'HEAD').stdout.strip()[:8]} — "
                f"refusing to move a tag"
            )
        print(f"  tag {tag} already exists at HEAD, nothing to create")
    else:
        msg = f"release: {repo.name} v{version}"
        print(f"  create annotated tag {tag} at HEAD: git tag -a {tag} -m {msg!r}")
        if not dry_run:
            git(repo, "tag", "-a", tag, "-m", msg)

    print(f"  push tag: git push origin {tag}")
    if not dry_run:
        run(["git", "-C", str(repo.path), "push", "origin", tag])
    print("  exec done\n")
    return tag


# ── Phase 3: watch ───────────────────────────────────────────────────────────

def watch_release(repo: Repo, tag: str, timeout_min: int) -> None:
    print("── watch ──")
    tag_sha = git(repo, "rev-parse", f"{tag}^{{}}").stdout.strip()
    deadline = time.time() + timeout_min * 60

    run_id = None
    while time.time() < deadline:
        runs = gh_json(
            repo, "run", "list", "--workflow", "release.yml",
            "--limit", "10", "--json", "databaseId,headSha,status,conclusion,displayTitle",
        )
        match = next((r for r in runs if r["headSha"] == tag_sha), None)
        if match:
            run_id = match["databaseId"]
            break
        time.sleep(5)

    if run_id is None:
        raise ReleaseError(
            f"{repo.name}: no release.yml run for {tag} ({tag_sha[:8]}) within "
            f"{timeout_min} min — the tag may not have triggered the workflow"
        )

    while time.time() < deadline:
        run = gh_json(repo, "run", "view", run_id,
                      "--json", "status,conclusion,displayTitle")[0]
        if run["status"] == "completed":
            if run["conclusion"] != "success":
                raise ReleaseError(
                    f"{repo.name}: release.yml run {run_id} concluded "
                    f"`{run['conclusion']}` — no Release was created"
                )
            print(f"  release.yml run {run_id} completed: success")
            print("  watch done\n")
            return
        print(f"  run {run_id}: {run['status']}…")
        time.sleep(15)

    raise ReleaseError(f"{repo.name}: release.yml run {run_id} did not finish "
                       f"within {timeout_min} min")


# ── Phase 4: verify (the whole point) ────────────────────────────────────────

def verify_release(repo: Repo, tag: str, assets: list[str]) -> None:
    print("── verify ──")
    tag_sha = git(repo, "rev-parse", f"{tag}^{{}}").stdout.strip()
    proc = gh(repo, "release", "view", tag, "--json",
              "tagName,isDraft,isPrerelease,url,publishedAt,targetCommitish,assets",
              check=False)
    if proc.returncode != 0:
        raise ReleaseError(
            f"{repo.name}: GitHub Release {tag} does NOT exist — "
            f"workflow ran but no release was created:\n  {proc.stderr.strip()}"
        )
    rel = json.loads(proc.stdout)

    checks = []
    checks.append(("Release exists", True))
    checks.append(("not a draft", rel["isDraft"] is False))
    checks.append(("not a prerelease", rel["isPrerelease"] is False))
    names = {a["name"] for a in rel["assets"]}
    missing = [a for a in assets if a not in names]
    checks.append(("all assets attached", not missing))
    checks.append(("target == tag commit", rel["targetCommitish"] == tag_sha))

    ok = True
    for label, passed in checks:
        mark = "✓" if passed else "✗"
        print(f"  {mark} {label}")
        ok = ok and passed
    print(f"  release: {rel['url']}")
    if missing:
        print(f"  ✗ missing assets: {missing}")
    if not ok:
        raise ReleaseError(f"{repo.name}: {tag} FAILED verification")
    print("  verify OK\n")


# ── main ─────────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Drive + verify the phi-agent GitHub Release.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__.split("── main")[0],
    )
    parser.add_argument("--version", help="release version, e.g. 0.11.2 (no leading v)")
    parser.add_argument("--check", action="store_true",
                        help="preflight only; no tag, no push")
    parser.add_argument("--verify", action="store_true",
                        help="skip preflight/exec/watch, just re-verify an existing tag")
    parser.add_argument("--push-commit", action="store_true",
                        help="also push HEAD to origin/<default branch> before tagging")
    parser.add_argument("--dry-run", action="store_true",
                        help="preflight + print the exec commands, execute nothing")
    parser.add_argument("--timeout-min", type=int, default=RELEASE_TIMEOUT_MIN,
                        help=f"max minutes to wait for release.yml (default {RELEASE_TIMEOUT_MIN})")
    parser.add_argument("--assets", default=",".join(PHI_AGENT_ASSETS),
                        help="expected release assets, comma-separated")
    parser.add_argument("-q", "--quiet", action="store_true",
                        help="suppress per-repo CI detail lines")
    args = parser.parse_args()
    verbose = not args.quiet

    try:
        verify_gh_cli()
        target = resolve_repo("phi-agent")

        if args.verify:
            if not args.version:
                parser.error("--verify requires --version")
            verify_release(target, f"v{args.version}", args.assets.split(","))
            print("✅ RELEASE VERIFIED")
            return 0

        if not args.version:
            parser.error("--version is required for the full flow (or use --verify)")

        if args.check:
            preflight(args.version, verbose)
            print("✅ PREFLIGHT PASSED")
            return 0

        preflight(args.version, verbose)
        tag = exec_release(target, args.version, args.push_commit, args.dry_run)
        if args.dry_run:
            print("✅ DRY RUN — nothing executed; tag would be pushed and then "
                  "watched + verified")
            return 0
        if not target.has_release_wf:
            print(f"  note: {target.name} has no release.yml — nothing to watch/verify "
                  "(library crates publish to crates.io separately)")
            print("✅ TAGGED (no GitHub Release for this repo)")
            return 0
        watch_release(target, tag, args.timeout_min)
        verify_release(target, tag, args.assets.split(","))
        print("✅ RELEASE VERIFIED")
        return 0

    except ReleaseError as exc:
        print(f"\n❌ {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
