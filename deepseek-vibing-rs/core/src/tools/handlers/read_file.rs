use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::resolve_tool_environment;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use deepseek_vibing_tools::READ_FILE_ALIAS_TOOL_NAME;
use deepseek_vibing_tools::READ_FILE_TOOL_NAME;
use deepseek_vibing_tools::ReadFileToolOptions;
use deepseek_vibing_tools::ToolName;
use deepseek_vibing_tools::ToolSpec;
use deepseek_vibing_tools::create_read_file_tool;
use deepseek_vibing_tools::parse_read_file_args;
use deepseek_vibing_tools::read_file_error_text;
use deepseek_vibing_tools::read_file_success_text;

#[derive(Clone)]
pub struct ReadFileHandler {
    tool_name: &'static str,
    options: ReadFileToolOptions,
}

impl ReadFileHandler {
    pub(crate) fn new_read_file(include_environment_id: bool) -> Self {
        Self {
            tool_name: READ_FILE_TOOL_NAME,
            options: ReadFileToolOptions {
                include_environment_id,
            },
        }
    }

    pub(crate) fn new_read_alias(include_environment_id: bool) -> Self {
        Self {
            tool_name: READ_FILE_ALIAS_TOOL_NAME,
            options: ReadFileToolOptions {
                include_environment_id,
            },
        }
    }

    fn tool_error(&self, kind: &str, message: impl AsRef<str>) -> FunctionCallError {
        FunctionCallError::RespondToModel(read_file_error_text(
            self.tool_name,
            kind,
            message.as_ref(),
        ))
    }
}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for ReadFileHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(self.tool_name)
    }

    fn spec(&self) -> ToolSpec {
        create_read_file_tool(self.options)
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
                return Err(self.tool_error(
                    "unsupported_payload",
                    format!("{} handler received unsupported payload", self.tool_name),
                ));
            }
        };
        let deepseek_vibing_tools::ReadFileArgs {
            path,
            start_line,
            limit,
            environment_id,
        } = parse_read_file_args(&arguments)
            .map_err(|err| self.tool_error(err.kind, err.message))?;

        let Some(turn_environment) =
            resolve_tool_environment(turn.as_ref(), environment_id.as_deref())?
        else {
            return Err(self.tool_error("unavailable", "read_file is unavailable in this session"));
        };
        let cwd = turn_environment.cwd.clone();
        let abs_path = cwd.join(&path);
        let sandbox = turn.file_system_sandbox_context(/*additional_permissions*/ None, &cwd);
        let fs = turn_environment.environment.get_filesystem();

        let metadata = fs
            .get_metadata(&abs_path, Some(&sandbox))
            .await
            .map_err(|error| {
                self.tool_error(
                    "not_found",
                    format!("unable to locate file at `{}`: {error}", abs_path.display()),
                )
            })?;

        if !metadata.is_file {
            return Err(self.tool_error(
                "not_file",
                format!("path `{}` is not a file", abs_path.display()),
            ));
        }

        let bytes = fs
            .read_file(&abs_path, Some(&sandbox))
            .await
            .map_err(|error| {
                self.tool_error(
                    "read_failed",
                    format!("unable to read file at `{}`: {error}", abs_path.display()),
                )
            })?;
        let size_bytes = bytes.len();
        let text = String::from_utf8(bytes).map_err(|err| {
            self.tool_error(
                "non_utf8",
                format!("file `{}` is not valid UTF-8: {err}", abs_path.display()),
            )
        })?;

        let text = read_file_success_text(
            abs_path.as_path().display().to_string(),
            &text,
            size_bytes,
            start_line,
            limit,
        )
        .map_err(|err| {
            self.tool_error(
                "serialization_failed",
                format!("failed to serialize read_file output: {err}"),
            )
        })?;

        Ok(boxed_tool_output(FunctionToolOutput::from_text(
            text,
            Some(true),
        )))
    }
}

impl CoreToolRuntime for ReadFileHandler {}
