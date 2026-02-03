# Exit command tests

# Test: exit from worktree removes only that worktree
echo "--- Test: exit from worktree ---"
_wt create exit-test1 2>/dev/null
_wt create exit-test2 2>/dev/null
# cd into worktree and run exit
cd "$TEST_DIR/.worktrees/exit-test1"
output=$(_wt exit 2>/dev/null)
cd "$TEST_DIR"  # return to base for assertions
assert_dir_not_exists "$TEST_DIR/.worktrees/exit-test1" "exit removed current worktree"
assert_dir_exists "$TEST_DIR/.worktrees/exit-test2" "exit preserved other worktree"
_wt remove exit-test2 2>/dev/null

# Test: exit outputs cd command to base repo
echo "--- Test: exit outputs cd to base ---"
_wt create exit-cd-test 2>/dev/null
cd "$TEST_DIR/.worktrees/exit-cd-test"
output=$(_wt exit 2>/dev/null)
cd "$TEST_DIR"
[[ "$output" == *"cd "* ]] && result="contains cd" || result="no cd"
assert_eq "contains cd" "$result" "exit outputs cd command"
# Handle macOS /var -> /private/var symlink
expected_path=$(cd "$TEST_DIR" && pwd -P)
[[ "$output" == *"$expected_path"* ]] && result="correct path" || result="wrong path"
assert_eq "correct path" "$result" "exit cd points to base repo"

# Test: exit from base repo should error
echo "--- Test: exit from base repo errors ---"
cd "$TEST_DIR"
output=$(_wt exit 2>&1) || exit_code=$?
[[ "$output" == *"Not in a worktree"* ]] && result="correct error" || result="wrong output"
assert_eq "correct error" "$result" "exit from base shows error"

# Test: exit without --force preserves dirty worktree
echo "--- Test: exit preserves dirty worktree ---"
_wt create exit-dirty-test 2>/dev/null
cd "$TEST_DIR/.worktrees/exit-dirty-test"
echo "uncommitted change" > testfile.txt
git add testfile.txt
# Regular exit should fail (uncommitted changes)
output=$(_wt exit 2>&1) || true
cd "$TEST_DIR"
# Worktree should still exist (removal failed)
if [[ -d "$TEST_DIR/.worktrees/exit-dirty-test" ]]; then
    echo -e "${GREEN}PASS${NC}: exit without --force preserves dirty worktree"
    ((PASS++))
else
    echo -e "${RED}FAIL${NC}: exit without --force removed dirty worktree"
    ((FAIL++))
fi

# Test: exit --force removes dirty worktree
echo "--- Test: exit --force removes dirty worktree ---"
cd "$TEST_DIR/.worktrees/exit-dirty-test" 2>/dev/null || cd "$TEST_DIR"
if [[ -d "$TEST_DIR/.worktrees/exit-dirty-test" ]]; then
    cd "$TEST_DIR/.worktrees/exit-dirty-test"
    _wt exit --force 2>/dev/null
    cd "$TEST_DIR"
fi
assert_dir_not_exists "$TEST_DIR/.worktrees/exit-dirty-test" "exit --force removes dirty worktree"

# Test: exit from nested worktree
echo "--- Test: exit from nested worktree ---"
_wt create feature/nested/test 2>/dev/null
cd "$TEST_DIR/.worktrees/feature/nested/test"
output=$(_wt exit 2>/dev/null)
cd "$TEST_DIR"
assert_dir_not_exists "$TEST_DIR/.worktrees/feature/nested/test" "exit removed nested worktree"

# Test: full create, open, exit flow
echo "--- Test: create, open, exit flow ---"
_wt create flow-test 2>/dev/null
assert_dir_exists "$TEST_DIR/.worktrees/flow-test" "create made worktree"
cd "$TEST_DIR"
# Test open outputs cd command
open_output=$(_wt open flow-test 2>/dev/null)
[[ "$open_output" == *"cd "* && "$open_output" == *"flow-test"* ]] && result="open works" || result="open broken"
assert_eq "open works" "$result" "open outputs cd to worktree"
# Now cd there and exit
cd "$TEST_DIR/.worktrees/flow-test"
exit_output=$(_wt exit 2>/dev/null)
cd "$TEST_DIR"
[[ "$exit_output" == *"cd "* ]] && result="exit works" || result="exit broken"
assert_eq "exit works" "$result" "exit outputs cd to base"
assert_dir_not_exists "$TEST_DIR/.worktrees/flow-test" "exit removed worktree"
