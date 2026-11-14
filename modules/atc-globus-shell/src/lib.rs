// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// atc-globus-shell — Terminal, CLI, Command Processor
//! Terminal-Huelle von GlobusOS: Kommandoregister, Kommandozeile und Pipes.
/// Registry of built-in commands and their handlers.
pub mod cli_commands;
/// Splitting and chaining of piped commands.
pub mod pipe_system;
/// Command processor tying registry, history and completion together.
pub mod shell;
/// Prefix completion over the known command names.
pub mod shell_completion;
/// Bounded command history.
pub mod shell_history;

pub use cli_commands::CommandRegistry;
pub use pipe_system::PipeSystem;
pub use shell::Shell;
pub use shell_completion::AutoComplete;
pub use shell_history::ShellHistory;
