# Tools Directory

This directory contains utility scripts and tools for Tenebrium development.

## Available Tools

### delete-branches.sh

**Purpose**: Delete all remote branches except the `release` branch.

**Usage**:
```bash
./tools/delete-branches.sh
```

**What it does**:
1. Lists all remote branches
2. Shows which branches will be preserved (`release`, `main`, `master`)
3. Shows which branches will be deleted
4. Asks for confirmation (type `DELETE`)
5. Deletes the specified branches
6. Shows a summary of the operation

**Requirements**:
- Git credentials with push access to the repository
- Bash shell

**Safety**:
- Preserves `release`, `main`, and `master` branches
- Requires explicit confirmation before deletion
- Shows detailed output of each deletion operation

**See also**: `docs/BRANCH_DELETION_GUIDE.md` for more deletion methods including GitHub Actions.

### verify_vectors.py

Test vector verification script for transaction canonicalization.

**Usage**:
```bash
python tools/verify_vectors.py
```

See the main README.md for more information.
