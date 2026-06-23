import sys
from pathlib import Path

_EXAMPLES_ROOT = Path(__file__).resolve().parents[1]
if str(_EXAMPLES_ROOT) not in sys.path:
    sys.path.insert(0, str(_EXAMPLES_ROOT))

from _bootstrap import ensure_local_sdk_src, runtime_config, server_label

ensure_local_sdk_src()

from deepseek_vibing import DeepSeekVibing

with DeepSeekVibing(config=runtime_config()) as agent:
    print("Server:", server_label(agent.metadata))

    thread = agent.thread_start(model="gpt-5.4", config={"model_reasoning_effort": "high"})
    turn = thread.turn("Say hello in one sentence.")
    result = turn.run()

    print("Thread:", thread.id)
    print("Turn:", result.id)
    print("Text:", result.final_response.strip())
