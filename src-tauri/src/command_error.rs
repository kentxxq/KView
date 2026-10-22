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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_error_code() {
        let err = CommandError::from("file-not-found: path does not exist");
        assert_eq!(err.code, "file-not-found");
        assert_eq!(err.message, "path does not exist");
        assert!(err.recoverable);
    }

    #[test]
    fn fallbacks_unstructured_error() {
        let err = CommandError::from("something went wrong without code");
        assert_eq!(err.code, "desktop-command-failed");
        assert_eq!(err.message, "something went wrong without code");
        assert!(err.recoverable);
    }

    #[test]
    fn ignores_invalid_code_prefix() {
        let err = CommandError::from("INVALID_CODE: something went wrong");
        assert_eq!(err.code, "desktop-command-failed");
        assert_eq!(err.message, "INVALID_CODE: something went wrong");
    }

    #[test]
    fn serializes_to_camel_case_json() {
        let err = CommandError::from("permission-denied: access denied");
        let json = serde_json::to_string(&err).expect("serialize error");
        assert!(json.contains("\"code\":\"permission-denied\""));
        assert!(json.contains("\"message\":\"access denied\""));
        assert!(json.contains("\"recoverable\":true"));
    }
}

