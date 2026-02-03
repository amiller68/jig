//! Which command - show path to wt executable

use anyhow::Result;

pub fn run() -> Result<()> {
    let exe = std::env::current_exe()?;
    println!("{}", exe.display());
    Ok(())
}
