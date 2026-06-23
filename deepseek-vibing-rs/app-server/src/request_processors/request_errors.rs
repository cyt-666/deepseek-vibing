use super::*;

pub(super) fn environment_selection_error_message(err: DeepSeekVibingErr) -> String {
    match err {
        DeepSeekVibingErr::InvalidRequest(message) => message,
        err => err.to_string(),
    }
}
