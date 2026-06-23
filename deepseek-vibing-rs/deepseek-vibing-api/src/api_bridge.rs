use crate::TransportError;
use crate::error::ApiError;
use crate::rate_limits::parse_promo_message;
use crate::rate_limits::parse_rate_limit_for_limit;
use crate::rate_limits::parse_rate_limit_reached_type;
use base64::Engine;
use chrono::DateTime;
use chrono::Utc;
use deepseek_vibing_protocol::auth::PlanType;
use deepseek_vibing_protocol::error::DeepSeekVibingErr;
use deepseek_vibing_protocol::error::RetryLimitReachedError;
use deepseek_vibing_protocol::error::UnexpectedResponseError;
use deepseek_vibing_protocol::error::UsageLimitReachedError;
use http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;

pub fn map_api_error(err: ApiError) -> DeepSeekVibingErr {
    match err {
        ApiError::ContextWindowExceeded => DeepSeekVibingErr::ContextWindowExceeded,
        ApiError::QuotaExceeded => DeepSeekVibingErr::QuotaExceeded,
        ApiError::UsageNotIncluded => DeepSeekVibingErr::UsageNotIncluded,
        ApiError::Retryable { message, delay } => DeepSeekVibingErr::Stream(message, delay),
        ApiError::Stream(msg) => DeepSeekVibingErr::Stream(msg, None),
        ApiError::ServerOverloaded => DeepSeekVibingErr::ServerOverloaded,
        ApiError::Api { status, message } => {
            DeepSeekVibingErr::UnexpectedStatus(UnexpectedResponseError {
                status,
                body: message,
                url: None,
                cf_ray: None,
                request_id: None,
                identity_authorization_error: None,
                identity_error_code: None,
            })
        }
        ApiError::InvalidRequest { message } => DeepSeekVibingErr::InvalidRequest(message),
        ApiError::CyberPolicy { message } => DeepSeekVibingErr::CyberPolicy { message },
        ApiError::Transport(transport) => match transport {
            TransportError::Http {
                status,
                url,
                headers,
                body,
            } => {
                let body_text = body.unwrap_or_default();

                if status == http::StatusCode::SERVICE_UNAVAILABLE
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&body_text)
                    && matches!(
                        value
                            .get("error")
                            .and_then(|error| error.get("code"))
                            .and_then(serde_json::Value::as_str),
                        Some("server_is_overloaded" | "slow_down")
                    )
                {
                    return DeepSeekVibingErr::ServerOverloaded;
                }

                if status == http::StatusCode::BAD_REQUEST {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&body_text)
                        && let Some(error) = parsed.get("error")
                        && error.get("code").and_then(Value::as_str)
                            == Some(CYBER_POLICY_ERROR_CODE)
                    {
                        let message = error
                            .get("message")
                            .and_then(Value::as_str)
                            .filter(|message| !message.trim().is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| CYBER_POLICY_FALLBACK_MESSAGE.to_string());
                        DeepSeekVibingErr::CyberPolicy { message }
                    } else if body_text
                        .contains("The image data you provided does not represent a valid image")
                    {
                        DeepSeekVibingErr::InvalidImageRequest()
                    } else {
                        DeepSeekVibingErr::InvalidRequest(body_text)
                    }
                } else if status == http::StatusCode::INTERNAL_SERVER_ERROR {
                    DeepSeekVibingErr::InternalServerError
                } else if status == http::StatusCode::TOO_MANY_REQUESTS {
                    if let Ok(err) = serde_json::from_str::<UsageErrorResponse>(&body_text) {
                        if err.error.error_type.as_deref() == Some("usage_limit_reached") {
                            let limit_id = extract_header(headers.as_ref(), ACTIVE_LIMIT_HEADER);
                            let rate_limits = headers.as_ref().and_then(|map| {
                                parse_rate_limit_for_limit(map, limit_id.as_deref())
                            });
                            let promo_message = headers.as_ref().and_then(parse_promo_message);
                            let rate_limit_reached_type =
                                headers.as_ref().and_then(parse_rate_limit_reached_type);
                            let resets_at = err
                                .error
                                .resets_at
                                .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0));
                            return DeepSeekVibingErr::UsageLimitReached(UsageLimitReachedError {
                                plan_type: err.error.plan_type,
                                resets_at,
                                rate_limits: rate_limits.map(Box::new),
                                promo_message,
                                rate_limit_reached_type,
                            });
                        } else if err.error.error_type.as_deref() == Some("usage_not_included") {
                            return DeepSeekVibingErr::UsageNotIncluded;
                        }
                    }

                    DeepSeekVibingErr::RetryLimit(RetryLimitReachedError {
                        status,
                        request_id: extract_request_tracking_id(headers.as_ref()),
                    })
                } else {
                    DeepSeekVibingErr::UnexpectedStatus(UnexpectedResponseError {
                        status,
                        body: body_text,
                        url,
                        cf_ray: extract_header(headers.as_ref(), CF_RAY_HEADER),
                        request_id: extract_request_id(headers.as_ref()),
                        identity_authorization_error: extract_header(
                            headers.as_ref(),
                            X_OPENAI_AUTHORIZATION_ERROR_HEADER,
                        ),
                        identity_error_code: extract_x_error_json_code(headers.as_ref()),
                    })
                }
            }
            TransportError::RetryLimit => DeepSeekVibingErr::RetryLimit(RetryLimitReachedError {
                status: http::StatusCode::INTERNAL_SERVER_ERROR,
                request_id: None,
            }),
            TransportError::Timeout => DeepSeekVibingErr::RequestTimeout,
            TransportError::Network(msg) | TransportError::Build(msg) => {
                DeepSeekVibingErr::Stream(msg, None)
            }
        },
        ApiError::RateLimit(msg) => DeepSeekVibingErr::Stream(msg, None),
    }
}

const ACTIVE_LIMIT_HEADER: &str = "x-deepseek-vibing-active-limit";
const REQUEST_ID_HEADER: &str = "x-request-id";
const OAI_REQUEST_ID_HEADER: &str = "x-oai-request-id";
const CF_RAY_HEADER: &str = "cf-ray";
const X_OPENAI_AUTHORIZATION_ERROR_HEADER: &str = "x-openai-authorization-error";
const X_ERROR_JSON_HEADER: &str = "x-error-json";
const CYBER_POLICY_ERROR_CODE: &str = "cyber_policy";
const CYBER_POLICY_FALLBACK_MESSAGE: &str =
    "This request has been flagged for possible cybersecurity risk.";

#[cfg(test)]
#[path = "api_bridge_tests.rs"]
mod tests;

fn extract_request_tracking_id(headers: Option<&HeaderMap>) -> Option<String> {
    extract_request_id(headers).or_else(|| extract_header(headers, CF_RAY_HEADER))
}

fn extract_request_id(headers: Option<&HeaderMap>) -> Option<String> {
    extract_header(headers, REQUEST_ID_HEADER)
        .or_else(|| extract_header(headers, OAI_REQUEST_ID_HEADER))
}

fn extract_header(headers: Option<&HeaderMap>, name: &str) -> Option<String> {
    headers.and_then(|map| {
        map.get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    })
}

fn extract_x_error_json_code(headers: Option<&HeaderMap>) -> Option<String> {
    let encoded = extract_header(headers, X_ERROR_JSON_HEADER)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let parsed = serde_json::from_slice::<Value>(&decoded).ok()?;
    parsed
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[derive(Debug, Deserialize)]
struct UsageErrorResponse {
    error: UsageErrorBody,
}

#[derive(Debug, Deserialize)]
struct UsageErrorBody {
    #[serde(rename = "type")]
    error_type: Option<String>,
    plan_type: Option<PlanType>,
    resets_at: Option<i64>,
}
