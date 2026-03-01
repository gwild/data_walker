#!/bin/bash
# After pulling, run this to commit .claude/skills/ to git
set -e
git add -f .claude/skills/
git commit -m "Track .claude/skills/ in git"
git push
echo "Done."
