"""Cargo builds for source-built DeepSeekVibing package artifacts."""

import os
import subprocess
from dataclasses import dataclass
from pathlib import Path

from .targets import REPO_ROOT
from .targets import PackageVariant
from .targets import TargetSpec
from .v8 import resolve_deepseek_vibing_v8_cargo_env


DEEPSEEK_VIBING_RS_ROOT = REPO_ROOT / "deepseek-vibing-rs"


@dataclass(frozen=True)
class SourceBuildOutputs:
    entrypoint_bin: Path
    bwrap_bin: Path | None
    deepseek_vibing_command_runner_bin: Path | None
    deepseek_vibing_windows_sandbox_setup_bin: Path | None


def build_source_binaries(
    spec: TargetSpec,
    variant: PackageVariant,
    *,
    cargo: str,
    profile: str,
    entrypoint_bin: Path | None,
    bwrap_bin: Path | None,
    deepseek_vibing_command_runner_bin: Path | None,
    deepseek_vibing_windows_sandbox_setup_bin: Path | None,
) -> SourceBuildOutputs:
    validate_prebuilt_resource_inputs(
        spec,
        bwrap_bin=bwrap_bin,
        deepseek_vibing_command_runner_bin=deepseek_vibing_command_runner_bin,
        deepseek_vibing_windows_sandbox_setup_bin=deepseek_vibing_windows_sandbox_setup_bin,
    )
    binaries = source_binaries_for_target(
        spec,
        variant,
        build_entrypoint=entrypoint_bin is None,
        build_bwrap=spec.is_linux and bwrap_bin is None,
        build_deepseek_vibing_command_runner=spec.is_windows and deepseek_vibing_command_runner_bin is None,
        build_deepseek_vibing_windows_sandbox_setup=spec.is_windows
        and deepseek_vibing_windows_sandbox_setup_bin is None,
    )
    if binaries:
        cmd = [
            cargo,
            "build",
            "--target",
            spec.target,
            "--profile",
            profile,
        ]
        for binary in binaries:
            cmd.extend(["--bin", binary])

        cargo_env = None
        if entrypoint_bin is None:
            deepseek_vibing_v8_env = resolve_deepseek_vibing_v8_cargo_env(spec)
            if deepseek_vibing_v8_env:
                cargo_env = {**os.environ, **deepseek_vibing_v8_env}

        print("+", " ".join(cmd))
        subprocess.run(
            cmd,
            cwd=DEEPSEEK_VIBING_RS_ROOT,
            check=True,
            env=cargo_env,
        )

    output_dir = cargo_profile_output_dir(spec, profile)
    outputs = SourceBuildOutputs(
        entrypoint_bin=resolve_output_path(
            entrypoint_bin,
            output_dir / variant.entrypoint_name(spec),
        ),
        bwrap_bin=resolve_output_path(
            bwrap_bin,
            output_dir / "bwrap" if spec.is_linux else None,
        ),
        deepseek_vibing_command_runner_bin=resolve_output_path(
            deepseek_vibing_command_runner_bin,
            output_dir / "deepseek-vibing-command-runner.exe" if spec.is_windows else None,
        ),
        deepseek_vibing_windows_sandbox_setup_bin=resolve_output_path(
            deepseek_vibing_windows_sandbox_setup_bin,
            output_dir / "deepseek-vibing-windows-sandbox-setup.exe" if spec.is_windows else None,
        ),
    )
    validate_source_outputs(outputs)
    return outputs


def source_binaries_for_target(
    spec: TargetSpec,
    variant: PackageVariant,
    *,
    build_entrypoint: bool,
    build_bwrap: bool,
    build_deepseek_vibing_command_runner: bool,
    build_deepseek_vibing_windows_sandbox_setup: bool,
) -> list[str]:
    binaries = []
    if build_entrypoint:
        binaries.append(variant.cargo_bin)
    if build_bwrap:
        binaries.append("bwrap")
    if build_deepseek_vibing_command_runner:
        binaries.append("deepseek-vibing-command-runner")
    if build_deepseek_vibing_windows_sandbox_setup:
        binaries.append("deepseek-vibing-windows-sandbox-setup")
    return binaries


def validate_prebuilt_resource_inputs(
    spec: TargetSpec,
    *,
    bwrap_bin: Path | None,
    deepseek_vibing_command_runner_bin: Path | None,
    deepseek_vibing_windows_sandbox_setup_bin: Path | None,
) -> None:
    if bwrap_bin is not None and not spec.is_linux:
        raise RuntimeError("--bwrap-bin is only supported for Linux targets.")
    if deepseek_vibing_command_runner_bin is not None and not spec.is_windows:
        raise RuntimeError(
            "--deepseek-vibing-command-runner-bin is only supported for Windows targets."
        )
    if deepseek_vibing_windows_sandbox_setup_bin is not None and not spec.is_windows:
        raise RuntimeError(
            "--deepseek-vibing-windows-sandbox-setup-bin is only supported for Windows targets."
        )


def resolve_output_path(
    explicit_path: Path | None, default_path: Path | None
) -> Path | None:
    if explicit_path is not None:
        return explicit_path.resolve()

    return default_path


def cargo_profile_output_dir(spec: TargetSpec, profile: str) -> Path:
    target_dir = cargo_target_dir()
    return target_dir / spec.target / cargo_profile_dirname(profile)


def cargo_target_dir() -> Path:
    target_dir = os.environ.get("CARGO_TARGET_DIR")
    if target_dir is None:
        return DEEPSEEK_VIBING_RS_ROOT / "target"

    path = Path(target_dir)
    if path.is_absolute():
        return path

    return DEEPSEEK_VIBING_RS_ROOT / path


def cargo_profile_dirname(profile: str) -> str:
    if profile == "dev":
        return "debug"
    if profile == "release":
        return "release"
    return profile


def validate_source_outputs(outputs: SourceBuildOutputs) -> None:
    for path in [
        outputs.entrypoint_bin,
        outputs.bwrap_bin,
        outputs.deepseek_vibing_command_runner_bin,
        outputs.deepseek_vibing_windows_sandbox_setup_bin,
    ]:
        if path is not None and not path.is_file():
            raise RuntimeError(f"cargo build did not produce expected binary: {path}")
