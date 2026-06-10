#!/usr/bin/env bash
# One-shot commit + push for routine changes (bug fixes, docs tweaks, screenshots).
#
# Usage:
#   ./scripts/publish.sh "fix: typo in README"
#   ./scripts/publish.sh "add main view screenshot"
#
# Or, without args (will prompt for commit message):
#   ./scripts/publish.sh
#
# What it does:
#   1. Shows what changed (git status --short)
#   2. Asks for confirmation
#   3. git add . && git commit -m "<message>"
#   4. git push
#
# What it does NOT do:
#   - Tag releases (use scripts/release.sh for that — to be added later when needed)
#   - Build the app (`npm run tauri build` is a separate concern)

set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "❌ 不在 git 仓库里" >&2
  exit 1
fi

if [ -z "$(git status --porcelain)" ]; then
  echo "✅ 没有任何改动，无需 push"
  exit 0
fi

echo "── 待提交的改动 ──"
git status --short
echo ""

MSG="${1:-}"
if [ -z "$MSG" ]; then
  read -rp "✏️  Commit 信息: " MSG
fi
if [ -z "$MSG" ]; then
  echo "❌ Commit 信息不能为空" >&2
  exit 1
fi

echo ""
echo "── 准备执行 ──"
echo "  git add ."
echo "  git commit -m \"$MSG\""
echo "  git push"
read -rp "确认？[y/N] " CONFIRM
case "$CONFIRM" in
  y|Y|yes|YES) ;;
  *) echo "已取消"; exit 0 ;;
esac

git add .
git commit -m "$MSG"
git push

echo ""
echo "✅ 推送完成"
echo "    查看：https://github.com/itsdrchen/Cento/commits/main"
