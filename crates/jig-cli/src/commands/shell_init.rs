//! Shell initialization command - prints shell integration code

use anyhow::Result;

const BASH_INIT: &str = r#"
# jig shell integration for bash
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}
"#;

const ZSH_INIT: &str = r#"
# jig shell integration for zsh
jig() {
    local output
    output=$(command jig "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}
"#;

const FISH_INIT: &str = r#"
# jig shell integration for fish
function jig
    set -l output (command jig $argv)
    set -l exit_code $status

    # Check if output starts with 'cd ' - if so, eval it
    if string match -q 'cd *' "$output"
        eval $output
    else if test -n "$output"
        echo $output
    end

    return $exit_code
end
"#;

pub fn run(shell: &str) -> Result<()> {
    let init_code = match shell.to_lowercase().as_str() {
        "bash" => BASH_INIT,
        "zsh" => ZSH_INIT,
        "fish" => FISH_INIT,
        _ => {
            eprintln!("Unsupported shell: {}", shell);
            eprintln!("Supported shells: bash, zsh, fish");
            std::process::exit(1);
        }
    };

    println!("{}", init_code.trim());
    Ok(())
}
