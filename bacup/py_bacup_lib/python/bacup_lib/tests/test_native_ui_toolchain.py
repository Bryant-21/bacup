from pathlib import Path
import re


def test_regen_ui_converters_do_not_use_external_flash_compilers():
    library = Path(__file__).resolve().parents[1]
    paths = list(library.glob("*_ui.py"))
    assert len(paths) >= 10
    forbidden = re.compile(r"\b(?:javaw?|jpexs|ffdec|mxmlc|playerglobal)\b|find_ffdec_jar", re.I)
    offenders = [path.name for path in paths if forbidden.search(path.read_text(encoding="utf-8"))]
    assert not offenders, offenders
