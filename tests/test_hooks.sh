# On-create hook tests

# Test: on-create hook set/get
echo "--- Test: on-create hook set/get ---"
_wt config on-create 'echo hello' 2>/dev/null
output=$(_wt config on-create 2>/dev/null)
assert_eq "echo hello" "$output" "on-create hook shows set value"

# Test: on-create hook shows in config
echo "--- Test: on-create shows in config ---"
output=$(_wt config 2>/dev/null)
[[ "$output" == *"On-create hook"* && "$output" == *"echo hello"* ]] && result="shown" || result="not shown"
assert_eq "shown" "$result" "on-create hook shown in config"

# Test: on-create hook runs on create
echo "--- Test: on-create hook runs ---"
_wt config on-create 'touch hook_ran.txt' 2>/dev/null
_wt create hook-test 2>/dev/null
[[ -f "$TEST_DIR/.worktrees/hook-test/hook_ran.txt" ]] && result="ran" || result="not ran"
assert_eq "ran" "$result" "on-create hook executed"
_wt remove hook-test 2>/dev/null

# Test: --no-hooks skips hook
echo "--- Test: --no-hooks skips hook ---"
_wt create no-hook-test --no-hooks 2>/dev/null
[[ ! -f "$TEST_DIR/.worktrees/no-hook-test/hook_ran.txt" ]] && result="skipped" || result="ran"
assert_eq "skipped" "$result" "--no-hooks skips hook"
_wt remove no-hook-test 2>/dev/null

# Test: on-create hook unset
echo "--- Test: on-create hook unset ---"
_wt config on-create --unset 2>/dev/null
output=$(_wt config on-create 2>/dev/null)
[[ "$output" == *"No on-create hook"* ]] && result="unset" || result="still set"
assert_eq "unset" "$result" "on-create hook --unset works"

# Test: config --list shows on-create hooks
echo "--- Test: config --list shows hooks ---"
_wt config on-create 'npm install' 2>/dev/null
output=$(_wt config --list 2>/dev/null)
[[ "$output" == *"on-create"* && "$output" == *"npm install"* ]] && result="correct" || result="missing"
assert_eq "correct" "$result" "config --list shows on-create hooks"
_wt config on-create --unset 2>/dev/null

# Test: hook failure still creates worktree
echo "--- Test: hook failure still creates worktree ---"
_wt config on-create 'exit 1' 2>/dev/null
_wt create fail-hook-test 2>/dev/null || true
assert_dir_exists "$TEST_DIR/.worktrees/fail-hook-test" "worktree created despite hook failure"
_wt remove fail-hook-test 2>/dev/null
_wt config on-create --unset 2>/dev/null
