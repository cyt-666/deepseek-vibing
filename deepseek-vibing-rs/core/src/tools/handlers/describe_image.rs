use deepseek_vibing_protocol::models::ImageDetail;
use deepseek_vibing_tools::ToolName;
use deepseek_vibing_tools::ToolSpec;
use serde::Deserialize;
use std::path::Path;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::describe_image_spec::DESCRIBE_IMAGE_TOOL_NAME;
use crate::tools::handlers::describe_image_spec::DescribeImageToolOptions;
use crate::tools::handlers::describe_image_spec::create_describe_image_tool;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use crate::vision_bridge;

pub struct DescribeImageHandler {
    options: DescribeImageToolOptions,
}

impl DescribeImageHandler {
    pub(crate) fn new(options: DescribeImageToolOptions) -> Self {
        Self { options }
    }
}

#[derive(Deserialize)]
struct DescribeImageArgs {
    path: String,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default)]
    task_hint: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for DescribeImageHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(DESCRIBE_IMAGE_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_describe_image_tool(self.options)
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
    }

    async fn handle(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn crate::tools::context::ToolOutput>, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "describe_image handler received unsupported payload".to_string(),
                ));
            }
        };
        let DescribeImageArgs {
            path,
            environment_id,
            task_hint,
            detail,
        } = parse_arguments(&arguments)?;
        let detail = match detail.as_deref() {
            None | Some("auto") => Some(ImageDetail::Auto),
            Some("low") => Some(ImageDetail::Low),
            Some("high") => Some(ImageDetail::High),
            Some(detail) => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "describe_image.detail only supports `auto`, `low`, or `high`; got `{detail}`"
                )));
            }
        };

        let observation = vision_bridge::observe_local_image_path(
            turn.as_ref(),
            Path::new(&path),
            environment_id.as_deref(),
            detail,
            task_hint.as_deref(),
        )
        .await;
        let ok = observation.ok;
        Ok(boxed_tool_output(FunctionToolOutput::from_text(
            observation.to_json_string(),
            Some(ok),
        )))
    }
}

impl CoreToolRuntime for DescribeImageHandler {}
