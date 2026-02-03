# Configuration tests: base branch, global config

# Test: config - default base branch
echo "--- Test: config default ---"
output=$(_wt config 2>/dev/null)
[[ "$output" == *"origin/main"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "default base branch is origin/main"

# Test: config - set repo base branch
echo "--- Test: config set repo base ---"
_wt config base origin/develop 2>/dev/null
output=$(_wt config base 2>/dev/null)
assert_eq "origin/develop" "$output" "config base shows set value"

# Test: config - get_base_branch returns repo config
echo "--- Test: get_base_branch uses repo config ---"
output=$(_wt config 2>/dev/null)
[[ "$output" == *"origin/develop"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "effective base branch uses repo config"

# Test: config - unset repo base branch
echo "--- Test: config unset repo base ---"
_wt config base --unset 2>/dev/null
output=$(_wt config base 2>/dev/null)
[[ "$output" == *"No config set"* ]] && result="unset" || result="still set"
assert_eq "unset" "$result" "config base --unset works"

# Test: config - set global default
echo "--- Test: config set global default ---"
_wt config base --global origin/master 2>/dev/null
output=$(_wt config base --global 2>/dev/null)
assert_eq "origin/master" "$output" "global default shows set value"

# Test: config - global is used when no repo config
echo "--- Test: config global fallback ---"
output=$(_wt config 2>/dev/null)
[[ "$output" == *"origin/master"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "global default is used as fallback"

# Test: config - repo config takes precedence over global
echo "--- Test: config repo over global ---"
_wt config base origin/feature 2>/dev/null
output=$(_wt config 2>/dev/null)
[[ "$output" == *"Effective base branch"*"origin/feature"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "repo config takes precedence over global"

# Test: config --list
echo "--- Test: config --list ---"
output=$(_wt config --list 2>/dev/null)
[[ "$output" == *"[global]"* && "$output" == *"$TEST_DIR"* ]] && result="correct" || result="wrong"
assert_eq "correct" "$result" "config --list shows all entries"

# Test: config - unset global
echo "--- Test: config unset global ---"
_wt config base --global --unset 2>/dev/null
output=$(_wt config base --global 2>/dev/null)
[[ "$output" == *"No global default"* ]] && result="unset" || result="still set"
assert_eq "unset" "$result" "config base --global --unset works"

# Clean up repo config for next tests
_wt config base --unset 2>/dev/null
