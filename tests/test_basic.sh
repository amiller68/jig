# Basic worktree operations: create, list, open, remove

# Test: create worktree
echo "--- Test: create worktree ---"
_wt create test1 2>/dev/null
assert_dir_exists "$TEST_DIR/.worktrees/test1" "create worktree"
# .git in worktree is a file, not a dir
[[ -f "$TEST_DIR/.worktrees/test1/.git" ]] && echo -e "${GREEN}PASS${NC}: worktree has .git file" && ((PASS++)) || { echo -e "${RED}FAIL${NC}: worktree missing .git file"; ((FAIL++)); }

# Test: list worktrees
echo "--- Test: list worktrees ---"
output=$(_wt list 2>/dev/null)
assert_eq "test1" "$output" "list shows test1"

# Test: create with -o flag outputs cd command
echo "--- Test: create with -o outputs cd ---"
output=$(_wt create test2 -o 2>/dev/null)
[[ "$output" == *"cd "* ]] && result="contains cd" || result="no cd"
assert_eq "contains cd" "$result" "create -o outputs cd command"

# Test: open outputs cd command
echo "--- Test: open outputs cd ---"
output=$(_wt open test1 2>/dev/null)
# Handle macOS /var -> /private/var symlink
expected_path=$(cd "$TEST_DIR/.worktrees/test1" && pwd -P)
[[ "$output" == *"$expected_path"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "open outputs correct cd"

# Test: remove worktree
echo "--- Test: remove worktree ---"
_wt remove test1 2>/dev/null
assert_dir_not_exists "$TEST_DIR/.worktrees/test1" "remove deletes worktree"

# Test: list after remove
echo "--- Test: list after remove ---"
output=$(_wt list 2>/dev/null)
[[ "$output" != *"test1"* ]] && result="test1 gone" || result="test1 still there"
assert_eq "test1 gone" "$result" "list doesn't show removed worktree"

# Test: create -o with error should not leak color codes to stdout
echo "--- Test: create -o error no color leak ---"
# Create a worktree first
_wt create errortest 2>/dev/null
# Try to create same one with -o, capture stdout only
stdout_output=$(_wt create errortest -o 2>/dev/null) || true
# Check stdout doesn't contain escape sequences (color codes)
[[ "$stdout_output" != *$'\033'* ]] && result="clean" || result="has color codes"
assert_eq "clean" "$result" "create -o error has no color codes in stdout"
# Cleanup
_wt remove errortest 2>/dev/null
