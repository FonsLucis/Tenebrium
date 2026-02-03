# Branch Deletion Guide

> **한국어**: release 브랜치를 제외한 모든 브랜치 삭제 가이드  
> **English**: Guide to delete all branches except the release branch

## Overview

This guide provides multiple methods to delete all branches except the `release` branch from the Tenebrium repository.

## Current Branches

As of the last check, the following branches exist:
- `codex/test-project-mining-setup`
- `codex/test-project-mining-setup-svf3c4`
- `codex/test-project-mining-setup-vhybjw`
- `copilot/create-todo-list-file`
- `copilot/delete-all-branches-except-release` (current working branch)
- `p2p-binary`
- **`release`** ← This will be preserved

## Method 1: Using GitHub Actions (Recommended)

A GitHub Actions workflow has been created at `.github/workflows/delete-branches.yml`.

### Steps:
1. Go to the repository on GitHub
2. Navigate to **Actions** tab
3. Select **"Delete All Branches Except Release"** workflow
4. Click **"Run workflow"**
5. Type `DELETE` in the confirmation input
6. Click **"Run workflow"** button

The workflow will automatically:
- Delete all branches except `release`, `main`, and `master`
- Provide a summary of deleted branches
- Show remaining branches

## Method 2: Using the Shell Script

A shell script has been created at `tools/delete-branches.sh`.

### Steps:

```bash
# Navigate to the repository
cd /path/to/Tenebrium

# Run the deletion script
./tools/delete-branches.sh
```

The script will:
1. Show current branch
2. List branches to preserve
3. List branches to delete
4. Ask for confirmation (type `DELETE`)
5. Delete the branches
6. Show a summary

### Requirements:
- Git credentials with push access to the repository
- Bash shell

## Method 3: Manual Deletion via Command Line

If you prefer manual control, use these commands:

```bash
# List all remote branches
git ls-remote --heads origin

# Delete specific branches (one at a time)
git push origin --delete branch-name

# Or delete multiple branches
git push origin --delete \
  codex/test-project-mining-setup \
  codex/test-project-mining-setup-svf3c4 \
  codex/test-project-mining-setup-vhybjw \
  copilot/create-todo-list-file \
  p2p-binary
```

## Method 4: Using GitHub CLI

If you have GitHub CLI (`gh`) installed:

```bash
# List all branches
gh api repos/FonsLucis/Tenebrium/branches --paginate | jq -r '.[].name'

# Delete branches (example for one branch)
gh api -X DELETE repos/FonsLucis/Tenebrium/git/refs/heads/branch-name

# Script to delete all except release
for branch in $(gh api repos/FonsLucis/Tenebrium/branches --paginate | jq -r '.[].name'); do
  if [ "$branch" != "release" ] && [ "$branch" != "main" ] && [ "$branch" != "master" ]; then
    echo "Deleting: $branch"
    gh api -X DELETE "repos/FonsLucis/Tenebrium/git/refs/heads/$branch"
  fi
done
```

## Verification

After deletion, verify the remaining branches:

```bash
# Via git
git ls-remote --heads origin

# Via GitHub CLI
gh api repos/FonsLucis/Tenebrium/branches --paginate | jq -r '.[].name'
```

You should only see the `release` branch (and possibly `main` or `master` if they existed).

## Safety Features

All provided methods preserve:
- `release` branch (primary target)
- `main` branch (if exists)
- `master` branch (if exists)

## Rollback

⚠️ **Warning**: Branch deletion is permanent and cannot be easily undone.

If you need to recover a deleted branch:
1. Find the commit SHA of the branch tip (from your local git reflog or GitHub)
2. Recreate the branch: `git push origin <commit-sha>:refs/heads/<branch-name>`

## Notes

- The current working branch (`copilot/delete-all-branches-except-release`) may remain until this PR is merged
- All deletion methods require appropriate permissions (push/admin access)
- It's recommended to backup important branches before deletion

## Support

For issues or questions, please refer to the repository maintainers.
