//! Attach command - attach to tmux session

use anyhow::Result;

use wt_core::spawn;

pub fn run(name: Option<&str>) -> Result<()> {
    spawn::attach(name)?;
    Ok(())
}
