// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Autocomplete
/// Prefix completion over a list of known command names.
pub struct AutoComplete {
    commands: Vec<String>,
}
impl Default for AutoComplete {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoComplete {
    /// Starts with the built-in command names.
    pub fn new() -> Self {
        Self {
            commands: ["echo", "help", "ls", "pwd", "whoami", "clear", "version"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
    /// Returns every known command that starts with `partial`.
    pub fn complete(&self, partial: &str) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| c.starts_with(partial))
            .cloned()
            .collect()
    }
    /// Adds a command name to the completion list.
    pub fn add(&mut self, cmd: &str) {
        self.commands.push(cmd.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_complete() {
        let ac = AutoComplete::new();
        let matches = ac.complete("e");
        assert!(matches.contains(&"echo".to_string()));
        let no_match = ac.complete("zzz");
        assert!(no_match.is_empty());
    }
}
