"""WAV <-> XWM <-> FUZ format conversion for game audio files."""
from __future__ import annotations

import logging
import os
import subprocess
import tempfile
from pathlib import Path

_log = logging.getLogger("toolkit.voice_changer.converter")

# Tool paths — resolved via app_paths for frozen/dev compatibility
from ui.toolkit.app_paths import get_resource_dir as _get_resource_dir
from creation_lib.paths import get_resource_dir as _get_creation_lib_resource_dir
_RESOURCE = _get_resource_dir()
_CREATION_LIB_RESOURCE = _get_creation_lib_resource_dir()
_XWMA_ENCODE = _CREATION_LIB_RESOURCE / "xWMAEncode.exe"
_FUZ_ENCODE = _CREATION_LIB_RESOURCE / "BmlFuzEncode.exe"
_FUZ_DECODE = _RESOURCE / "BmlFuzDecode.exe"


def detect_format(path: str) -> str:
    """Detect audio format from extension. Returns 'wav', 'xwm', or 'fuz'."""
    ext = os.path.splitext(path)[1].lower()
    if ext == ".wav":
        return "wav"
    elif ext == ".xwm":
        return "xwm"
    elif ext == ".fuz":
        return "fuz"
    else:
        raise ValueError(f"Unsupported audio format: {ext}")


def to_wav(input_path: str, output_dir: str | None = None) -> str:
    """Convert XWM or FUZ to WAV. Returns path to the WAV file.

    If input is already WAV, returns the input path unchanged.

    Args:
        input_path: Path to input audio file.
        output_dir: Directory for output. Defaults to same dir as input.

    Returns:
        Path to the WAV file.
    """
    fmt = detect_format(input_path)
    if fmt == "wav":
        return input_path

    if output_dir is None:
        output_dir = os.path.dirname(input_path)

    base = os.path.splitext(os.path.basename(input_path))[0]
    wav_path = os.path.join(output_dir, f"{base}.wav")

    if fmt == "fuz":
        # FUZ -> WAV (via BmlFuzDecode, which extracts WAV + LIP)
        _run_tool(_FUZ_DECODE, [input_path, wav_path])
    elif fmt == "xwm":
        # XWM -> WAV (via xWMAEncode in decode mode)
        _run_tool(_XWMA_ENCODE, [input_path, wav_path])

    if not os.path.isfile(wav_path):
        raise RuntimeError(f"Conversion failed — output not created: {wav_path}")

    _log.info("Converted %s -> %s", input_path, wav_path)
    return wav_path


def from_wav(wav_path: str, target_format: str, output_dir: str | None = None) -> str:
    """Convert WAV back to XWM or FUZ. Returns path to the output file.

    Args:
        wav_path: Path to input WAV file.
        target_format: 'xwm' or 'fuz'.
        output_dir: Directory for output. Defaults to same dir as input.

    Returns:
        Path to the converted file.
    """
    if target_format == "wav":
        return wav_path

    if output_dir is None:
        output_dir = os.path.dirname(wav_path)

    base = os.path.splitext(os.path.basename(wav_path))[0]

    if target_format == "xwm":
        xwm_path = os.path.join(output_dir, f"{base}.xwm")
        _run_tool(_XWMA_ENCODE, [wav_path, xwm_path])
        return xwm_path

    elif target_format == "fuz":
        # WAV -> XWM -> FUZ
        xwm_path = os.path.join(output_dir, f"{base}.xwm")
        fuz_path = os.path.join(output_dir, f"{base}.fuz")
        _run_tool(_XWMA_ENCODE, [wav_path, xwm_path])
        _run_tool(_FUZ_ENCODE, [xwm_path, fuz_path])
        # Clean up intermediate XWM
        if os.path.isfile(xwm_path):
            os.remove(xwm_path)
        return fuz_path

    else:
        raise ValueError(f"Unsupported target format: {target_format}")


def _run_tool(tool_path: Path, args: list[str]) -> None:
    """Run an external encoding/decoding tool."""
    if not tool_path.is_file():
        raise FileNotFoundError(f"Tool not found: {tool_path}")

    cmd = [str(tool_path)] + args
    _log.debug("Running: %s", " ".join(cmd))
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        _log.error("Tool failed (%d): %s\nstderr: %s", result.returncode, " ".join(cmd), result.stderr)
        raise RuntimeError(f"Tool failed with exit code {result.returncode}: {result.stderr}")
