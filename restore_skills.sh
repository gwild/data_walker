#!/bin/bash
# Run this on the origin machine after pulling to restore skills to git tracking.
#
# What this does:
#   1. Checks that .claude/skills/ exists locally (from before the gitignore fix)
#   2. Force-adds the skills to git (overriding any old .gitignore caching)
#   3. Commits and pushes

set -e

echo "=== Restore .claude/skills/ to git tracking ==="

# Verify we're in the repo root
if [ ! -f CLAUDE.md ]; then
    echo "ERROR: Run this from the data_walker repo root."
    exit 1
fi

# Pull latest (includes the .gitignore fix)
echo "Pulling latest..."
git pull

# Check if skills exist locally
if [ ! -d .claude/skills ]; then
    echo "ERROR: .claude/skills/ not found on this machine."
    echo "If skills were lost, you'll need to recreate them."
    exit 1
fi

echo "Found skills:"
find .claude/skills -type f | sort

# Force-add skills (needed because .claude/ was previously gitignored)
echo ""
echo "Adding skills to git..."
git add -f .claude/skills/

# Also add settings.json if it exists (team-shared config)
if [ -f .claude/settings.json ]; then
    echo "Adding .claude/settings.json..."
    git add -f .claude/settings.json
fi

# Show what will be committed
echo ""
echo "Staged changes:"
git diff --cached --stat

# Commit and push
echo ""
read -p "Commit and push? [y/N] " confirm
if [ "$confirm" = "y" ] || [ "$confirm" = "Y" ]; then
    git commit -m "Track .claude/skills/ in git

Previously the blanket .claude/ gitignore excluded shared skills.
Now only local/personal files are ignored."
    git push
    echo "Done! Skills are now tracked in git."
else
    echo "Aborted. Changes are staged but not committed."
fi
