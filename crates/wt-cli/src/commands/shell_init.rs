//! Shell initialization command - prints shell integration code

use anyhow::Result;

const BASH_INIT: &str = r#"
# scribe shell integration for bash
scribe() {
    local output
    output=$(command scribe "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}

# Short alias
sc() {
    scribe "$@"
}
"#;

const ZSH_INIT: &str = r#"
# scribe shell integration for zsh
scribe() {
    local output
    output=$(command scribe "$@")
    local exit_code=$?

    # Check if output starts with 'cd ' - if so, eval it
    if [[ "$output" == cd\ * ]]; then
        eval "$output"
    elif [[ -n "$output" ]]; then
        echo "$output"
    fi

    return $exit_code
}

# Short alias
sc() {
    scribe "$@"
}
"#;

const FISH_INIT: &str = r#"
# scribe shell integration for fish
function scribe
    set -l output (command scribe $argv)
    set -l exit_code $status

    # Check if output starts with 'cd ' - if so, eval it
    if string match -q 'cd *' "$output"
        eval $output
    else if test -n "$output"
        echo $output
    end

    return $exit_code
end

# Short alias
function sc
    scribe $argv
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
