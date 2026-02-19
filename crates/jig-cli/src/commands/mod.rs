//! CLI command implementations

pub mod attach;
pub mod config;
pub mod create;
pub mod exit;
pub mod health;
pub mod init;
pub mod kill;
pub mod list;
pub mod merge;
pub mod open;
pub mod ps;
pub mod remove;
pub mod review;
pub mod shell_init;
pub mod shell_setup;
pub mod spawn;
pub mod status;
pub mod update;
pub mod version;
pub mod which;

// Re-export command structs
pub use attach::Attach;
pub use config::Config;
pub use create::Create;
pub use exit::Exit;
pub use health::Health;
pub use init::Init;
pub use kill::Kill;
pub use list::List;
pub use merge::Merge;
pub use open::Open;
pub use ps::Ps;
pub use remove::Remove;
pub use review::Review;
pub use shell_init::ShellInit;
pub use shell_setup::ShellSetup;
pub use spawn::Spawn;
pub use status::Status;
pub use update::Update;
pub use version::Version;
pub use which::Which;
