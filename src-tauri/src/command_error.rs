use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandError {
    code: String,
    message: String,
    recoverable: bool,
}

impl CommandError {
    fn from_message(message: String) -> Self {
        let (code, message) = message
            .split_once(": ")
            .filter(|(prefix, _)| {
                !prefix.is_empty()
                    && prefix.chars().all(|character| character.is_ascii_lowercase() || character == '-')
            })
            .map(|(code, message)| (code.to_string(), message.to_string()))
            .unwrap_or_else(|| ("desktop-command-failed".into(), message));
        Self { code, message, recoverable: true }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::from_message(message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::from_message(message.to_string())
    }
}

pub(crate) type CommandResult<T> = Result<T, CommandError>;
