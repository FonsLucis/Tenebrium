#!/bin/bash
# Script to delete all branches except release branch
# Usage: ./delete-branches.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Branch Deletion Script ===${NC}"
echo ""

# Get current branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
echo "Current branch: $CURRENT_BRANCH"
echo ""

# Branches to preserve
PRESERVE_BRANCHES=("release" "main" "master")
echo -e "${GREEN}Branches to preserve:${NC}"
for branch in "${PRESERVE_BRANCHES[@]}"; do
    echo "  - $branch"
done
echo ""

# Fetch all remote branches
echo "Fetching remote branches..."
git fetch --all --prune
echo ""

# Get all remote branches
BRANCHES=$(git ls-remote --heads origin | awk '{print $2}' | sed 's|refs/heads/||')

# List branches that will be deleted
TO_DELETE=()
TO_PRESERVE=()

for branch in $BRANCHES; do
    should_delete=true
    
    # Check if branch should be preserved
    for preserve in "${PRESERVE_BRANCHES[@]}"; do
        if [ "$branch" == "$preserve" ]; then
            should_delete=false
            TO_PRESERVE+=("$branch")
            break
        fi
    done
    
    if [ "$should_delete" = true ]; then
        TO_DELETE+=("$branch")
    fi
done

# Display summary
echo -e "${GREEN}Branches to preserve (${#TO_PRESERVE[@]}):${NC}"
for branch in "${TO_PRESERVE[@]}"; do
    echo "  ✓ $branch"
done
echo ""

if [ ${#TO_DELETE[@]} -eq 0 ]; then
    echo -e "${GREEN}No branches to delete!${NC}"
    exit 0
fi

echo -e "${RED}Branches to delete (${#TO_DELETE[@]}):${NC}"
for branch in "${TO_DELETE[@]}"; do
    echo "  ✗ $branch"
done
echo ""

# Confirm deletion
echo -e "${YELLOW}WARNING: This will permanently delete ${#TO_DELETE[@]} branch(es) from the remote repository!${NC}"
read -p "Type 'DELETE' to confirm: " confirmation

if [ "$confirmation" != "DELETE" ]; then
    echo -e "${RED}Deletion cancelled.${NC}"
    exit 1
fi

echo ""
echo "Starting deletion..."
echo ""

# Delete branches
DELETED=0
FAILED=0

for branch in "${TO_DELETE[@]}"; do
    echo -n "Deleting $branch... "
    if git push origin --delete "$branch" 2>&1; then
        echo -e "${GREEN}OK${NC}"
        ((DELETED++))
    else
        echo -e "${RED}FAILED${NC}"
        ((FAILED++))
    fi
done

echo ""
echo -e "${GREEN}=== Summary ===${NC}"
echo "Deleted: $DELETED"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
fi
echo ""

# List remaining branches
echo "Remaining remote branches:"
git ls-remote --heads origin | awk '{print $2}' | sed 's|refs/heads/||' | while read branch; do
    echo "  - $branch"
done
