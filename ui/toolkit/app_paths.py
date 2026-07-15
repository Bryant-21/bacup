"""UI path resolution shim."""
from app.paths import (  # noqa: F401
    is_frozen,
    get_app_root,
    get_exe_dir,
    get_code_root,
    get_resource_dir,
    get_db_dir,
    get_settings_path,
    get_settings_config_dir,
    get_shared_settings_path,
    get_variant_settings_path,
    get_logs_dir,
    get_ini_dir,
)
