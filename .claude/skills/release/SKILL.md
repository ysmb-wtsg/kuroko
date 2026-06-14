---
name: release
description: >-
  Release a new version of kuroko end-to-end: bump the workspace version, commit, tag, push,
  create a GitHub Release, and update the Homebrew tap formula. Use this whenever the user asks to
  cut a release, ship a version, publish, or "version up" — including Japanese phrasings like
  「リリースして」「バージョンを上げて」「バージョンアップ」「公開して」. Trigger even when the user only
  says "release" or names a target version (e.g. "0.3.0 でリリース"). This is a kuroko-specific
  workflow that pushes tags and creates public releases, so always follow these exact steps rather
  than improvising the git/gh/brew commands.
---

# kuroko release

Cut a kuroko release. A release is **three artifacts that must all ship together**: an annotated
git tag, a GitHub Release, and an updated Homebrew formula. Skipping any one leaves users on a
stale or inconsistent version, so run the whole sequence to the end.

There is no CI release automation — this is manual. The steps below are ordered by dependency
(e.g. the formula's sha256 can only be computed after the tag tarball exists on GitHub).

## Repo facts

- Version is defined in **one place**: `Cargo.toml` `[workspace.package]` → `version`. Crates
  inherit it via `version.workspace = true`. Never edit per-crate versions.
- Main repo: `ysmb-wtsg/kuroko` (you are in it). Default branch `main`.
- Homebrew tap: separate repo `ysmb-wtsg/homebrew-tap`, cloned at `/tmp/homebrew-tap`. Formula at
  `Formula/kuroko.rb`. If the clone is missing, `git clone git@github.com:ysmb-wtsg/homebrew-tap.git /tmp/homebrew-tap`.
- Public-facing text (tag summary, release notes) is **English**. Commit messages follow
  Conventional Commits.

## Step 0 — Decide the version and check the tree

1. **Determine the new version.**
   - If the user passed a version (arg or in their message, e.g. `0.3.0`), use it verbatim.
   - Otherwise, look at commits since the last tag (`git log $(git describe --tags --abbrev=0)..HEAD --oneline`)
     and **ask the user** which part to bump, recommending one based on Conventional Commits
     (feat → minor, fix/chore/docs → patch; pre-1.0 this project has historically used patch even
     for features — surface that and let them choose). Use AskUserQuestion.
2. **Check for unrelated uncommitted work.** Run `git status`. If there are modified/untracked
   files that are *not* part of this release, do not silently sweep them in — the tarball is built
   from the committed tag, so uncommitted work will be **excluded** from the release. Tell the user
   what's uncommitted and confirm whether to proceed (the release will not contain it) or commit it
   first. Exclude editor/OS junk (`.DS_Store`) and unrelated scratch files.
3. Confirm the feature work being released is already committed.

Let `X.Y.Z` be the chosen version below.

## Step 1 — Bump version and commit

```bash
# Edit Cargo.toml [workspace.package] version -> "X.Y.Z" (use the Edit tool, not sed, to be precise)
cargo update -w          # sync Cargo.lock to the new version
git add Cargo.toml Cargo.lock
git commit -m "chore(release): bump version to X.Y.Z"
```

Sanity check the lock picked it up: `grep -A1 'name = "kuroko"' Cargo.lock`.

## Step 2 — Push, tag, push tag

The pre-push hook runs `cargo clippy --workspace`; a failure here blocks the release — fix it, don't bypass.

```bash
git push origin main
git tag -a vX.Y.Z -m "vX.Y.Z: <one-line English summary of the headline change>"
git push origin vX.Y.Z
```

Tag message format is exactly `vX.Y.Z: <summary>`.

## Step 3 — GitHub Release (always create one)

Write English release notes from the commits since the previous tag — group by Features / Fixes,
mention user-visible behavior and any new config. Reference an ADR if the release added one.

```bash
gh release create vX.Y.Z \
  --title "kuroko vX.Y.Z" \
  --notes "$(cat <<'EOF'
## Features
- ...

## Fixes
- ...
EOF
)"
```

The command prints the release URL — keep it for the final summary.

## Step 4 — Update the Homebrew formula

The tag tarball now exists, so its sha256 is computable. The formula's `version` is derived from
the `url` tag, so update **both** `url` and `sha256`.

```bash
SHA=$(curl -sL https://github.com/ysmb-wtsg/kuroko/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256 | awk '{print $1}')
echo "$SHA"   # sanity: 64 hex chars

cd /tmp/homebrew-tap && git pull --quiet
# In Formula/kuroko.rb, set:
#   url   ".../archive/refs/tags/vX.Y.Z.tar.gz"
#   sha256 "$SHA"
# (Edit tool preferred; verify with: grep -nE 'url|sha256' Formula/kuroko.rb)
git -C /tmp/homebrew-tap add Formula/kuroko.rb
git -C /tmp/homebrew-tap commit -m "kuroko X.Y.Z"
git -C /tmp/homebrew-tap push
```

Formula commit message is exactly `kuroko X.Y.Z` (no `v`, no Conventional Commit prefix — that's
the tap's convention).

## Step 5 — Report

Summarize: new version, the bump commit hash, the tag, the GitHub Release URL, and the formula
commit. If you warned about uncommitted work in Step 0, restate that it was **not** included.

## Cautions

- Tag push and release creation are **irreversible / outward-facing**. Don't dry-run them; run them
  once the version and notes are confirmed.
- Don't bump or release if the working tree's release-relevant changes aren't committed — the
  release is built from the tag, not the working tree.
- Keep the memory `project_kuroko_release` in sync if the process changes (repo names, tap path,
  conventions).
