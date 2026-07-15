"""Tests for shader #include preprocessor and #define injection."""
from __future__ import annotations

import pytest

from creation_lib.renderer.shader_pipeline import resolve_includes, inject_defines


class TestResolveIncludes:
    def test_no_includes_returns_unchanged(self):
        src = "void main() { gl_FragColor = vec4(1.0); }"
        assert resolve_includes(src, {}) == src

    def test_resolves_single_include(self):
        includes = {"includes/common.glsl": "// common code\n"}
        src = '#include "includes/common.glsl"\nvoid main() {}'
        result = resolve_includes(src, includes)
        assert "// common code" in result
        assert "#include" not in result

    def test_resolves_nested_includes(self):
        includes = {
            "includes/a.glsl": '#include "includes/b.glsl"\n// a code',
            "includes/b.glsl": "// b code\n",
        }
        src = '#include "includes/a.glsl"\nvoid main() {}'
        result = resolve_includes(src, includes)
        assert "// b code" in result
        assert "// a code" in result

    def test_circular_include_raises(self):
        includes = {
            "includes/a.glsl": '#include "includes/b.glsl"',
            "includes/b.glsl": '#include "includes/a.glsl"',
        }
        with pytest.raises(ValueError, match="[Cc]ircular"):
            resolve_includes('#include "includes/a.glsl"', includes)

    def test_missing_include_raises(self):
        with pytest.raises(ValueError, match="not found"):
            resolve_includes('#include "missing.glsl"', {})

    def test_multiple_includes(self):
        includes = {
            "a.glsl": "// a\n",
            "b.glsl": "// b\n",
        }
        src = '#include "a.glsl"\n#include "b.glsl"\nvoid main() {}'
        result = resolve_includes(src, includes)
        assert "// a" in result
        assert "// b" in result
        assert "#include" not in result

    def test_preserves_non_include_lines(self):
        src = '#version 330\n#include "a.glsl"\nuniform float x;'
        includes = {"a.glsl": "// inserted\n"}
        result = resolve_includes(src, includes)
        assert "#version 330" in result
        assert "uniform float x;" in result


class TestInjectDefines:
    def test_define_injection_after_version(self):
        src = "#version 330\nvoid main() {}"
        defines = {"HAS_PALETTE": None, "METALLIC_ROUGHNESS": None}
        result = inject_defines(src, defines)
        assert "#define HAS_PALETTE" in result
        assert "#define METALLIC_ROUGHNESS" in result
        lines = result.split("\n")
        version_idx = next(i for i, l in enumerate(lines) if "#version" in l)
        define_idx = next(i for i, l in enumerate(lines) if "#define HAS_PALETTE" in l)
        assert define_idx > version_idx

    def test_define_with_value(self):
        src = "#version 330\nvoid main() {}"
        defines = {"MAX_LIGHTS": "4"}
        result = inject_defines(src, defines)
        assert "#define MAX_LIGHTS 4" in result

    def test_no_defines_returns_unchanged(self):
        src = "#version 330\nvoid main() {}"
        assert inject_defines(src, {}) == src

    def test_no_version_line_prepends(self):
        src = "void main() {}"
        defines = {"FOO": None}
        result = inject_defines(src, defines)
        assert "#define FOO" in result
        assert "void main()" in result
