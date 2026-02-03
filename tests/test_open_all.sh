# Tests for open --all flag and health command

# Force unknown terminal during tests to avoid opening real tabs
export TERM_PROGRAM="test-runner"
unset KITTY_WINDOW_ID
unset WEZTERM_UNIX_SOCKET

# Test: health command runs without error
echo "--- Test: health command ---"
output=$(_wt health 2>&1)
exit_code=$?
assert_eq "0" "$exit_code" "health command exits successfully"
[[ "$output" == *"Terminal:"* ]] && result="has terminal" || result="missing terminal"
assert_eq "has terminal" "$result" "health shows terminal info"
[[ "$output" == *"Dependencies:"* ]] && result="has deps" || result="missing deps"
assert_eq "has deps" "$result" "health shows dependencies"

# Test: open --all with no worktrees
echo "--- Test: open --all with no worktrees ---"
# First ensure we have a clean slate - remove all worktrees
# Remove known leftovers from other tests plus use glob pattern
_wt remove test2 --force 2>/dev/null || true
_wt remove 'test*' --force 2>/dev/null || true
_wt remove 'hook*' --force 2>/dev/null || true
_wt remove 'fail*' --force 2>/dev/null || true
_wt remove 'error*' --force 2>/dev/null || true
_wt remove 'exit*' --force 2>/dev/null || true
_wt remove 'flow*' --force 2>/dev/null || true
_wt remove 'a/*' --force 2>/dev/null || true
_wt remove 'feature/*' --force 2>/dev/null || true
# Verify no worktrees left
output=$(_wt open --all 2>&1) || true
[[ "$output" == *"No worktrees"* ]] && result="shows message" || result="no message"
assert_eq "shows message" "$result" "open --all shows no worktrees message"

# Test: open --all doesn't output cd command (tabs opened directly)
echo "--- Test: open --all no cd output ---"
_wt create openall-test1 2>/dev/null
_wt create openall-test2 2>/dev/null
stdout_output=$(_wt open --all 2>/dev/null) || true
# stdout should be empty or not contain cd (since tabs are opened directly)
[[ -z "$stdout_output" || "$stdout_output" != *"cd "* ]] && result="no cd" || result="has cd"
assert_eq "no cd" "$result" "open --all doesn't output cd to stdout"

# Test: open --all reports warning for unsupported terminal
echo "--- Test: open --all unsupported terminal ---"
output=$(_wt open --all 2>&1)
# Should contain warning about unsupported terminal (since TERM_PROGRAM=test-runner)
[[ "$output" == *"Warning"* || "$output" == *"not supported"* ]] && result="warns" || result="no warning"
assert_eq "warns" "$result" "open --all warns about unsupported terminal"

# Cleanup
_wt remove openall-test1 2>/dev/null || true
_wt remove openall-test2 2>/dev/null || true

# Test: detect_terminal function exists and returns something
echo "--- Test: detect_terminal function ---"
# The function is internal, but we can check health output for terminal detection
output=$(_wt health 2>&1)
# Should show a terminal type (even if "unknown") - strip ANSI codes for matching
stripped=$(echo "$output" | sed 's/\x1b\[[0-9;]*m//g')
[[ "$stripped" == *"Terminal: "* ]] && result="detected" || result="not detected"
assert_eq "detected" "$result" "terminal detection works"
