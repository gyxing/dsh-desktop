use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

const TRUNCATION_MARKER: &str = "\n…[已截断]";

#[derive(Clone, Copy)]
pub enum DiagnosticSource {
    System,
    Stderr,
}

impl DiagnosticSource {
    fn label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Stderr => "stderr",
        }
    }
}

struct DiagnosticEntry {
    timestamp_millis: u128,
    generation: u64,
    source: DiagnosticSource,
    message: String,
}

/// 保存当前进程内最近的脱敏诊断，避免把 DSH 输出持久化到磁盘。
pub struct DiagnosticBuffer {
    entries: VecDeque<DiagnosticEntry>,
    message_bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl DiagnosticBuffer {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            message_bytes: 0,
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(TRUNCATION_MARKER.len()),
        }
    }

    /// 在进入共享缓冲前完成脱敏和截断，复制诊断时不再接触原始凭据。
    pub fn push(&mut self, generation: u64, source: DiagnosticSource, message: &str) {
        let redacted = redact_message(message);
        let message = truncate_message(&redacted, self.max_bytes);

        while self.entries.len() >= self.max_entries
            || self.message_bytes + message.len() > self.max_bytes
        {
            let Some(removed) = self.entries.pop_front() else {
                break;
            };
            self.message_bytes = self.message_bytes.saturating_sub(removed.message.len());
        }

        self.message_bytes += message.len();
        self.entries.push_back(DiagnosticEntry {
            timestamp_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            generation,
            source,
            message,
        });
    }

    pub fn snapshot(&self) -> String {
        if self.entries.is_empty() {
            return "无可用诊断信息".to_string();
        }

        self.entries
            .iter()
            .map(|entry| {
                format!(
                    "[{}] generation={} source={} {}",
                    entry.timestamp_millis,
                    entry.generation,
                    entry.source.label(),
                    entry.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn redact_message(message: &str) -> String {
    message
        .lines()
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str) -> String {
    let lowercase = line.to_ascii_lowercase();
    let contains_secret_name = ["authorization", "api_key", "apikey", "token"]
        .iter()
        .any(|name| lowercase.contains(name));

    if contains_secret_name {
        if let Some(separator) = line.find([':', '=']) {
            return format!(
                "{}{} [已脱敏]",
                &line[..separator],
                &line[separator..=separator]
            );
        }
        return "[已脱敏]".to_string();
    }

    redact_sk_tokens(line)
}

fn redact_sk_tokens(line: &str) -> String {
    let mut redacted = String::new();
    for word in line.split_whitespace() {
        if !redacted.is_empty() {
            redacted.push(' ');
        }
        if word.starts_with("sk-") {
            redacted.push_str("[已脱敏]");
        } else {
            redacted.push_str(word);
        }
    }
    redacted
}

fn truncate_message(message: &str, max_bytes: usize) -> String {
    if message.len() <= max_bytes {
        return message.to_string();
    }

    let prefix_budget = max_bytes.saturating_sub(TRUNCATION_MARKER.len());
    let mut boundary = prefix_budget.min(message.len());
    while boundary > 0 && !message.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}{}", &message[..boundary], TRUNCATION_MARKER)
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticBuffer, DiagnosticSource};

    #[test]
    fn diagnostics_redact_credentials_before_they_can_be_copied() {
        let mut diagnostics = DiagnosticBuffer::new(10, 1024);

        diagnostics.push(
            7,
            DiagnosticSource::Stderr,
            "Authorization: Bearer secret-value\napi_key=sk-private-value\ntoken=token-value",
        );

        let snapshot = diagnostics.snapshot();
        assert!(!snapshot.contains("secret-value"));
        assert!(!snapshot.contains("sk-private-value"));
        assert!(!snapshot.contains("token-value"));
        assert!(snapshot.contains("[已脱敏]"));
    }

    #[test]
    fn diagnostics_discard_the_oldest_entry_when_the_entry_limit_is_reached() {
        let mut diagnostics = DiagnosticBuffer::new(2, 1024);

        diagnostics.push(1, DiagnosticSource::System, "first-entry");
        diagnostics.push(1, DiagnosticSource::System, "second-entry");
        diagnostics.push(2, DiagnosticSource::Stderr, "third-entry");

        let snapshot = diagnostics.snapshot();
        assert!(!snapshot.contains("first-entry"));
        assert!(snapshot.contains("second-entry"));
        assert!(snapshot.contains("third-entry"));
        assert!(snapshot.contains("generation=2"));
    }

    #[test]
    fn diagnostics_truncate_one_oversized_message_instead_of_exceeding_the_budget() {
        let mut diagnostics = DiagnosticBuffer::new(10, 32);

        diagnostics.push(3, DiagnosticSource::Stderr, &"x".repeat(128));

        let snapshot = diagnostics.snapshot();
        assert!(snapshot.contains("[已截断]"));
        assert!(!snapshot.contains(&"x".repeat(64)));
    }
}
