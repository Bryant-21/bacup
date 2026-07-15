import bacup_lib.models as models


def test_conversion_context_has_no_creation_handle_bridge() -> None:
    assert not hasattr(models.ConversionContext, "target_handle_native")
    assert not hasattr(models, "_BorrowedNativeHandle")
