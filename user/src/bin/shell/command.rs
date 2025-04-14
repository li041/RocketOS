use alloc::{
    ffi::CString,
    string::{String, ToString},
    vec::Vec,
};
use user_lib::{execve, exit};

pub struct Command {
    tokens: Vec<CString>,
}

impl From<&str> for Command {
    fn from(line: &str) -> Self {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut in_quote = None; // None = not in quote, Some('"') or Some('\'') = in quote
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                // Entering a quote
                '"' | '\'' if in_quote.is_none() => {
                    in_quote = Some(c);
                }
                // Exiting a quote
                '"' | '\'' if in_quote == Some(c) => {
                    in_quote = None;
                }
                // Handle spaces (only split if not in a quote)
                ' ' if in_quote.is_none() => {
                    if !current_token.is_empty() {
                        tokens.push(current_token);
                        current_token = String::new();
                    }
                }
                // Handle escape sequences (e.g., \n, \t, \", \\)
                '\\' if in_quote.is_some() => {
                    if let Some(next_c) = chars.next() {
                        match next_c {
                            'n' => current_token.push('\n'),
                            't' => current_token.push('\t'),
                            'r' => current_token.push('\r'),
                            '"' => current_token.push('"'),
                            '\'' => current_token.push('\''),
                            '\\' => current_token.push('\\'),
                            _ => {
                                // Unknown escape sequence, treat as literal (e.g., `\x` -> `x`)
                                current_token.push(next_c);
                            }
                        }
                    }
                }
                // Default: add character to current token
                _ => {
                    current_token.push(c);
                }
            }
        }

        // Add the last token if it exists
        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        // Convert Vec<String> to Vec<CString>
        let tokens = tokens
            .into_iter()
            .map(|s| CString::new(s).unwrap())
            .collect();
        Command { tokens }
    }
}

impl Command {
    pub fn get_name(&self) -> &str {
        self.tokens[0].to_str().unwrap()
    }

    /// excluding the command name
    pub fn get_args(&self) -> Vec<&str> {
        if self.tokens.len() < 2 {
            return Vec::new();
        }
        self.tokens[1..]
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect()
    }

    /// including the command name
    pub fn get_argv(&self) -> Vec<&str> {
        self.tokens.iter().map(|s| s.to_str().unwrap()).collect()
    }

    pub fn exec(&self) {
        execve(self.get_name(), &self.get_argv(), &[]);
        exit(-1);
    }
}
