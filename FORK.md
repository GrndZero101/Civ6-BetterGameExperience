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
