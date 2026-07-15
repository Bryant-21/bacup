"""Python context-manager wrapper around a native ConversionRun."""
from __future__ import annotations

from typing import Any


def _native() -> Any:
    from bacup_lib.native_runtime import load_native_module
    return load_native_module()


class ConversionRun:
    """A single mod-conversion run.

    Wraps the Rust-side ``ConversionRun`` registry entry. Implements the
    context-manager protocol so the run is always dropped (even on error).

    Plugin handles are private to the BACUP native module and owned by the run.
    """

    def __init__(self, run_id: int) -> None:
        self._id = run_id
        self._closed = False

    @classmethod
    def create_new(
        cls,
        source_game: str,
        target_game: str,
        source_plugin_path: str | None,
        target_plugin_name: str,
        *,
        master_plugin_paths: list[str] | tuple[str, ...] = (),
        source_strings_dir: str | None = None,
        config: dict[str, Any] | None = None,
    ) -> "ConversionRun":
        run_id = _native().conversion_run_create_from_paths(
            source_game,
            target_game,
            source_plugin_path,
            target_plugin_name,
            None,
            list(master_plugin_paths),
            source_strings_dir,
            config,
        )
        return cls(run_id)

    @classmethod
    def open_existing(
        cls,
        source_game: str,
        target_game: str,
        source_plugin_path: str | None,
        target_plugin_path: str,
        *,
        master_plugin_paths: list[str] | tuple[str, ...] = (),
        source_strings_dir: str | None = None,
        config: dict[str, Any] | None = None,
    ) -> "ConversionRun":
        run_id = _native().conversion_run_create_from_paths(
            source_game,
            target_game,
            source_plugin_path,
            None,
            target_plugin_path,
            list(master_plugin_paths),
            source_strings_dir,
            config,
        )
        return cls(run_id)

    # ------------------------------------------------------------------
    # Context-manager protocol
    # ------------------------------------------------------------------

    def __enter__(self) -> "ConversionRun":
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except Exception:
            pass

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def id(self) -> int:
        """The integer run ID used in all native calls."""
        return self._id

    def close(self) -> None:
        if self._closed:
            return
        _native().conversion_run_drop(self._id)
        self._closed = True

    def save_target(
        self,
        output_path: str | None = None,
        *,
        emit_authoring_yaml: bool = False,
        run_nvnm_validator: bool = True,
    ) -> None:
        _native().conversion_run_save_target(
            self._id, output_path, emit_authoring_yaml, run_nvnm_validator
        )

    def release_source_handle(self) -> bool:
        return _native().conversion_run_release_source_handle(self._id)

    def set_target_description(self, text: str) -> None:
        _native().conversion_run_set_target_description(self._id, text)

    def collect_lod_closures(
        self, root_form_keys: list[str] | tuple[str, ...] = ()
    ) -> list[tuple[str, str, str]]:
        return list(
            _native().conversion_run_collect_lod_closures(
                self._id, list(root_form_keys)
            )
        )

    def release_master_handles(self) -> int:
        return _native().conversion_run_release_master_handles(self._id)

    def drain_decisions(self) -> list[dict[str, str]]:
        """Drain and return all accumulated decisions as a list of dicts.

        Each dict has keys ``"kind"`` (str) and ``"message"`` (str).
        After this call the run's decision buffer is empty.
        """
        return _native().conversion_run_drain_decisions(self._id)

    def drain_warnings(self) -> list[str]:
        """Drain and return all accumulated warnings as a list of strings.

        After this call the run's warning buffer is empty.
        """
        return _native().conversion_run_drain_warnings(self._id)

    def run_phase(
        self,
        name: str,
        *,
        mod_path: str,
        source_extracted_dir: str = "",
        target_extracted_dir: str | None = None,
        target_data_dir: str | None = None,
        params: dict | None = None,
    ) -> dict:
        """Dispatch a named phase. Returns the PhaseReport as a dict.

        The dispatch params dict is built here and passed to native;
        callers shouldn't construct it themselves.
        """
        payload = {
            "mod_path": mod_path,
            "source_extracted_dir": source_extracted_dir,
            "params": params or {},
        }
        if target_extracted_dir is not None:
            payload["target_extracted_dir"] = target_extracted_dir
        if target_data_dir is not None:
            payload["target_data_dir"] = target_data_dir
        return _native().conversion_run_phase(self._id, name, payload)

    def drain_events(self, max: int = 256) -> list[dict]:
        """Drain up to ``max`` queued PhaseEvent dicts. Non-blocking."""
        return _native().conversion_run_drain_events(self._id, max)

    def cancel(self) -> None:
        """Request cancellation. Phases check this at safe points."""
        _native().conversion_run_cancel(self._id)

    def release_remap_state(self) -> None:
        """Release the FK remap state (MapperState) to free ~1–3 GB of RSS.

        Safe to call after fixups complete; the mapper is not needed by asset
        phases or ESP serialisation.
        """
        _native().conversion_run_release_remap_state(self._id)

    @staticmethod
    def list_phases() -> list[str]:
        """Return every phase name registered in the native dispatcher."""
        return _native().conversion_run_list_phases()
