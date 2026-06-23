"""Python SDK for running DeepSeekVibing workflows.

Start with :class:`DeepSeekVibing` for synchronous applications or
:class:`AsyncDeepSeekVibing` for async applications. Most programs create a thread and
run a turn::

    from deepseek_vibing import DeepSeekVibing, Sandbox

    with DeepSeekVibing() as agent:
        thread = agent.thread_start(sandbox=Sandbox.workspace_write)
        result = thread.run("Describe this project.")
        print(result.final_response)
"""

from ._version import __version__
from .api import (
    ApprovalMode,
    AsyncChatgptLoginHandle,
    AsyncDeepSeekVibing,
    AsyncDeviceCodeLoginHandle,
    AsyncThread,
    AsyncTurnHandle,
    ChatgptLoginHandle,
    DeepSeekVibing,
    DeviceCodeLoginHandle,
    ImageInput,
    Input,
    InputItem,
    LocalImageInput,
    MentionInput,
    RunInput,
    Sandbox,
    SkillInput,
    TextInput,
    Thread,
    TurnHandle,
    TurnResult,
)
from .client import DeepSeekVibingConfig
from .errors import (
    DeepSeekVibingError,
    DeepSeekVibingRpcError,
    InternalRpcError,
    InvalidParamsError,
    InvalidRequestError,
    JsonRpcError,
    MethodNotFoundError,
    ParseError,
    RetryLimitExceededError,
    ServerBusyError,
    TransportClosedError,
    is_retryable_error,
)
from .retry import retry_on_overload

__all__ = [
    "__version__",
    "DeepSeekVibingConfig",
    "DeepSeekVibing",
    "AsyncDeepSeekVibing",
    "ApprovalMode",
    "Sandbox",
    "ChatgptLoginHandle",
    "DeviceCodeLoginHandle",
    "AsyncChatgptLoginHandle",
    "AsyncDeviceCodeLoginHandle",
    "Thread",
    "AsyncThread",
    "TurnHandle",
    "AsyncTurnHandle",
    "TurnResult",
    "Input",
    "InputItem",
    "RunInput",
    "TextInput",
    "ImageInput",
    "LocalImageInput",
    "SkillInput",
    "MentionInput",
    "retry_on_overload",
    "DeepSeekVibingError",
    "TransportClosedError",
    "JsonRpcError",
    "DeepSeekVibingRpcError",
    "ParseError",
    "InvalidRequestError",
    "MethodNotFoundError",
    "InvalidParamsError",
    "InternalRpcError",
    "ServerBusyError",
    "RetryLimitExceededError",
    "is_retryable_error",
]
