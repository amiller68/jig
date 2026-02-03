# Nested path and glob pattern tests

# Test: nested path create
echo "--- Test: nested path create ---"
_wt create feature/test/nested 2>/dev/null
assert_dir_exists "$TEST_DIR/.worktrees/feature/test/nested" "nested path created"

# Test: list shows nested path
echo "--- Test: list shows nested ---"
output=$(_wt list 2>/dev/null)
[[ "$output" == *"feature/test/nested"* ]] && result="found" || result="not found"
assert_eq "found" "$result" "list shows nested path"

# Test: regex remove
echo "--- Test: regex remove ---"
_wt create regex-test1 2>/dev/null
_wt create regex-test2 2>/dev/null
_wt remove 'regex-test*' 2>/dev/null
output=$(_wt list 2>/dev/null)
[[ "$output" != *"regex-test"* ]] && result="removed" || result="still exists"
assert_eq "removed" "$result" "regex remove works"
