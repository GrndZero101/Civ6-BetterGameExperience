# Fork notes: keeping `respin` up to date with upstream

This repo is a fork of [Goaterino/Civ6-BetterGameExperience](https://github.com/Goaterino/Civ6-BetterGameExperience).
The fork's own work lives on `respin` (and feature branches merged into it, like `feat/canal-hills`).
When Goaterino pushes new commits, we **rebase** our branches onto theirs so our changes stay as a
small, readable stack on top of upstream.

## The setup

| Remote     | Points to                          | Used for                     |
|------------|------------------------------------|------------------------------|
| `origin`   | `GrndZero101/Civ6-BetterGameExperience` | where we push our branches |
| `upstream` | `Goaterino/Civ6-BetterGameExperience`   | where we pull their changes |

| Branch             | Tracks                     | What it is                                     |
|--------------------|----------------------------|------------------------------------------------|
| `main`             | `upstream/main`            | a plain mirror of Goaterino's `main`, never commit here |
| `respin`           | `origin/respin`            | upstream + all fork changes                    |
| `feat/canal-hills` | `origin/feat/canal-hills`  | a feature branch, already merged into `respin` |

Check with `git remote -v` and `git branch -vv`.

## The routine (PowerShell)

### 1. Look at what's new

```powershell
git status                               # must be clean before starting
git fetch upstream
git log --oneline main..upstream/main    # Goaterino's new commits
git log --oneline --graph main..respin   # our fork commits that will be replayed
```

Optional: preview conflicts without touching anything. One line of output (a tree hash) means it's clean;
anything listing files means there will be conflicts to resolve.

```powershell
git merge-tree --write-tree --name-only respin upstream/main
```

### 2. Update the local `main` mirror

```powershell
git switch main
git merge --ff-only upstream/main
```

`--ff-only` refuses to do anything if `main` has commits upstream doesn't, which is a good alarm:
nothing should ever be committed on `main` here.

### 3. Make safety tags

```powershell
git tag respin-backup respin
git tag canal-backup feat/canal-hills
git tag old-base (git merge-base respin upstream/main)
```

Tags are just bookmarks on the current commits, so you can get back to exactly here whatever happens.
`old-base` marks the upstream commit `respin` is currently built on, used to compare in step 5.

### 4. Rebase both branches in one go

```powershell
git switch respin
git rebase --rebase-merges --update-refs upstream/main
```

What the flags do:

- **`--rebase-merges`**: keeps the "Merge feat/canal-hills" merge commits instead of flattening
  everything into a straight line.
- **`--update-refs`**: any other branch pointing at a commit being rebased (here `feat/canal-hills`)
  is moved to the new copy of that commit. Without it, `feat/canal-hills` would be left behind on the
  old commits. Needs Git 2.38 or later (`git --version`).

At the end Git prints the branches it updated; both `respin` and `feat/canal-hills` should appear.

### 5. Verify

```powershell
# The fork commits now sit on top of upstream, same shape as before
git log --oneline --graph upstream/main..respin

# Each fork commit should show "=" (identical) between old and new
git range-diff old-base..respin-backup upstream/main..respin

# feat/canal-hills is still inside respin (prints 0)
git merge-base --is-ancestor feat/canal-hills respin; $LASTEXITCODE

# Everything that changed should be upstream's work, not ours
git diff respin-backup respin --stat

# Fork version string is still there
Select-String -Path proxy/src/version.rs -Pattern 'pub const VERSION'
```

Stronger check: our own changes should be identical before and after. These two diffs should show
the same edits (the `index` and `@@` lines may differ, that's fine):

```powershell
git diff old-base respin-backup --stat
git diff upstream/main respin --stat
```

Then run the tests:

```powershell
cd proxy; cargo test; cd ..
cd startup_proxy; cargo test; cd ..
```

```bash
# Lua tests (Git Bash)
for f in proxy/tests/test_*.lua startup_proxy/tests/test_*.lua; do lua "$f" || echo "FAIL: $f"; done
```

### 6. Push

A rebase rewrites commits (new hashes), so the push has to be forced:

```powershell
git push --force-with-lease origin respin feat/canal-hills
```

`--force-with-lease` (rather than `--force`) refuses to push if GitHub has commits you haven't fetched,
so you can't silently wipe out something pushed from elsewhere.

### 7. Clean up

```powershell
git tag -d respin-backup canal-backup old-base
git branch -vv     # every branch should show [origin/...] or [upstream/...] with no ahead/behind
```

## If something goes wrong

### A conflict stops the rebase

Git pauses and lists the conflicted files.

```powershell
git status                 # shows "both modified" files
# edit each file: pick the right content between <<<<<<< ======= >>>>>>> and delete the markers
git add <file>
git rebase --continue      # repeat until done
```

Bail out completely and go back to before step 4:

```powershell
git rebase --abort
```

**Likely conflict: `proxy/src/version.rs`.** The fork sets `VERSION` to `"<upstream version>-respin.N"`
so fork builds never join stock BGE lobbies. When Goaterino bumps the version, both sides edit that line.
Resolve by taking **upstream's new number plus our suffix**, e.g. upstream goes to `"0.11.0"` → we use
`"0.11.0-respin.1"`. Keep the `// FORK` comment above it.

### Upstream rewrote its history (force-pushed `main`)

It has happened: on 18/11/26 every upstream commit after 6 Nov came back with a new hash and the
same content. The signs are a huge "new" count with conflicts in files we never touched, and step 2
refusing (`git merge --ff-only` says "Not possible to fast-forward"). Do **not** run step 4 as
written: a plain rebase replays all of upstream's old commits on top of their new copies.

`uv run scripts/fork_status.py`, in the civ6-tools workspace, spots this. It names the old upstream tip
in our history, its same-tree twin in the new one, and the true count of new commits, then prints
the rebase to run. By hand the check looks like this, with `main` still on the old upstream tip:

```powershell
git cherry -v respin upstream/main | Select-String '^\+'   # the commits that are really new
git diff --stat main <new commit with the same subject>    # empty: same content, new hash
```

Then replace steps 2 and 4. `main` only ever mirrors upstream, so moving it loses nothing. Tag it
first anyway (step 3, plus `git tag main-backup main`), and use the old tip as `old-base`:

```powershell
git tag old-base main
git branch -f main upstream/main
git switch respin
git rebase --rebase-merges --update-refs --onto upstream/main old-base respin
```

Step 5's checks work unchanged, using `old-base` as above.

### `respin` built on an upstream branch that is not on `main` yet

**Resolved 21/11/26, kept as the recipe for next time.** Goaterino merged the branch into `main`
with `91974a4` intact (PR #20, then 0.13.0): case B below. `respin` went back onto `upstream/main`;
the only conflicts were the expected `patch.rs` rows (129 upstream + 3 `FORK` = 132) and
`version.rs` (`0.13.0-respin.1`), plus neighbouring insertions in `proxy/src/lib.rs` and
`proxy/README.md` (keep both). The `0.12.0-respin.2` commit was dropped: it only marked the lobby
change that upstream's 0.13.0 now carries. `stability-base` is deleted and `scripts/forks.toml`
watches `main` alone again.

**How it started (20/11/26).** Steam updated the game to 1.0.12.74 (build 26595377) and every BGE
build on `upstream/main` hung or crashed it. Goaterino's port was on `upstream/yodha-stability-fixes`
(one commit, `91974a4`), not on `main`, so `respin` was rebased onto that branch instead:

```powershell
git tag stability-base upstream/yodha-stability-fixes   # the upstream commit we sit on
git rebase --rebase-merges --update-refs upstream/yodha-stability-fixes
```

Fork additions on top for that build: our three `canaux.rs` addresses in the `MOVED` table of
`proxy/src/patch.rs` (rows marked `FORK`), `canaux.rs` going through `patch::rva`, and lobby
version `0.12.0-respin.2` (dropped at 0.13.0). Keep the tag `stability-base` until the branch
reaches `main`; `old-base`, `respin-backup` and `canal-backup` (the pre-rebase state on `0bcb7af`) can go once a
multiplayer game has been played on the new build.

`fork_status` watches both `main` and `yodha-stability-fixes` (`scripts/forks.toml`), so it reports
when either moves. When it does, find out which case it is:

```powershell
git fetch upstream
git merge-base --is-ancestor stability-base upstream/main; $LASTEXITCODE   # 0 = 91974a4 itself is on main
git log --oneline stability-base..upstream/yodha-stability-fixes          # new commits on the branch
```

**A. The branch got more commits, still not on `main`.** Move onto its new tip; only our commits
replay. Then move the tag.

```powershell
git rebase --rebase-merges --update-refs --onto upstream/yodha-stability-fixes stability-base respin
git tag -f stability-base upstream/yodha-stability-fixes
```

**B. Merged into `main` with its commits intact** (the check above prints 0). The normal routine
works unchanged: Git sees `91974a4` already on `upstream/main` and drops it.

```powershell
git switch main; git merge --ff-only upstream/main
git switch respin
git rebase --rebase-merges --update-refs upstream/main
```

**C. Squashed, rebased or amended into `main`** (prints 1, but `main` has the port: a new
`BUILD_1_0_12_74` / `MOVED` in `proxy/src/patch.rs`). Replay only our commits, never `91974a4`:

```powershell
git switch main; git merge --ff-only upstream/main
git switch respin
git rebase --rebase-merges --update-refs --onto upstream/main stability-base respin
```

**Every case, after the rebase** (on top of step 5's checks):

- `proxy/src/patch.rs`: the conflict to expect. Keep upstream's table and re-add our three `FORK`
  rows **in sorted order** (the search is binary; `moved_addresses` fails otherwise), and fix the
  array length. If upstream's `MOVED` is gone or renamed, the port changed shape: re-read
  `patch.rs` before porting the three addresses again (`canaux.rs` header says what they are).
- `proxy/src/version.rs`: upstream's new number plus our suffix, `-respin.1` again (e.g.
  `0.13.0-respin.1`); if upstream did not bump, still bump our suffix whenever the lobby or POL
  exchange changed.
- `cargo test` in both crates (the proxy's `every_address_of_the_sources_is_in_the_table` and
  `canaux::sites_du_jeu` read the installed GameCore), the Lua tests, and `check_hooks.py` against
  the installed `GameCore_XP2_FinalRelease.dll`: the log must show
  `canaux : test de colline redirigé` and no `version inattendue`.
- Build, both `install.ps1` (save `rollover_proxy.cfg` first: the GameCore installer rewrites it with
  defaults), check `BgdVersion=`, then a solo game past turn 3 before any online test.
- Once on `main`: `git tag -d stability-base`, and drop `yodha-stability-fixes` from `watch` in
  `scripts/forks.toml`.

### The rebase finished but the result looks wrong

```powershell
git switch respin
git reset --hard respin-backup
git branch -f feat/canal-hills canal-backup
```

This puts both branches back exactly where they were. (Only works while the backup tags still exist,
so verify before deleting them.)

### Already pushed and want to undo

Reset as above, then push the old commits back:

```powershell
git push --force-with-lease origin respin feat/canal-hills
```

## Alternative: merge instead of rebase

```powershell
git fetch upstream
git switch respin
git merge upstream/main
git push origin respin
```

No history rewrite and no force push, which matters if other people pull from `respin`. The downside is a
"Merge upstream" commit every sync, and our fork changes get interleaved with upstream's in the log, so
it's harder to see what the fork actually changes. Rebasing is the choice here because it's a personal
fork carrying a few patches.
