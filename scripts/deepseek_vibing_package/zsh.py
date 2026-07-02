"""Fetch the patched zsh fork used by shell_zsh_fork."""

from __future__ import annotations

from pathlib import Path

from .dotslash import fetch_dotslash_executable
from .targets import REPO_ROOT
from .targets import TargetSpec


ZSH_MANIFEST = REPO_ROOT / "scripts" / "deepseek_vibing_package" / "deepseek-vibing-zsh"
ZSH_RESOURCE_PATH = Path("zsh") / "bin" / "zsh"


def resolve_zsh_bin(spec: TargetSpec) -> Path | None:
    return fetch_dotslash_executable(
        spec,
        manifest_path=ZSH_MANIFEST,
        artifact_label="deepseek-vibing-zsh",
        cache_key=f"{spec.target}-zsh",
        dest_name="zsh",
        missing_ok=True,
    )
