// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// Command history
/// Bounded list of past commands; the oldest entry is dropped first.
pub struct ShellHistory {
    commands: Vec<String>,
    max: usize,
}
impl ShellHistory {
    /// Creates an empty history holding at most `max` entries.
    pub fn new(max: usize) -> Self {
        Self {
            commands: Vec::new(),
            max,
        }
    }
    /// Appends a command and drops the oldest entry when full.
    pub fn add(&mut self, cmd: &str) {
        if self.commands.len() >= self.max {
            self.commands.remove(0);
        }
        self.commands.push(cmd.into());
    }
    /// The `n` most recent commands, newest first.
    pub fn last(&self, n: usize) -> Vec<&String> {
        self.commands.iter().rev().take(n).collect()
    }
    /// All remembered commands containing `pattern`.
    pub fn search(&self, pattern: &str) -> Vec<&String> {
        self.commands
            .iter()
            .filter(|c| c.contains(pattern))
            .collect()
    }
    /// Drops all entries.
    pub fn clear(&mut self) {
        self.commands.clear();
    }
    /// Number of remembered commands.
    pub fn count(&self) -> usize {
        self.commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_history() {
        let mut h = ShellHistory::new(3);
        h.add("ls");
        h.add("pwd");
        h.add("echo hi");
        h.add("help");
        assert_eq!(h.count(), 3);
        assert_eq!(h.search("echo").len(), 1);
    }
}
