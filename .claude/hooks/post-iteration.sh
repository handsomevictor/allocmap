#!/bin/bash
# post-iteration.sh
# 每次迭代完成后自动执行
# 由 Orchestrator 在 Step 7 调用

set -e

PHASE=${1:-1}
ITER=${2:-"01"}
DESC=${3:-"iteration complete"}

echo "=========================================="
echo "Post-iteration hook: Phase ${PHASE} Iter ${ITER}"
echo "=========================================="

# 1. 最终编译验证
echo "[1/4] Final build verification..."
cargo build --release
echo "✓ Build passed"

# 2. 最终 clippy 检查
echo "[2/4] Final clippy check..."
cargo clippy -- -D warnings
echo "✓ Clippy: 0 warnings"

# 3. 最终测试
echo "[3/4] Final test run..."
cargo test --workspace
echo "✓ All tests passed"

# 4. Git push
echo "[4/4] Pushing to GitHub..."

if [ -z "$GITHUB_TOKEN" ]; then
    echo "ERROR: GITHUB_TOKEN not set"
    exit 1
fi

git config --global user.name "handsomevictor"
git config --global user.email "allocmap-dev@users.noreply.github.com"
git remote set-url origin https://oauth2:${GITHUB_TOKEN}@github.com/handsomevictor/allocmap.git

git add .
git commit -m "feat(phase${PHASE}): iter ${ITER} - ${DESC}

Reviewer: PASSED
Tester: PASSED
Clippy: 0 warnings" || echo "Nothing to commit"

git push origin main
echo "✓ Pushed to github.com/handsomevictor/allocmap"

echo "=========================================="
echo "Post-iteration hook complete"
echo "=========================================="
