# tests/fixtures

## `font.ttf`

A 96-glyph subset of [Lato](http://www.latofonts.com/) Regular renamed to
`PdFunTestFont` (the Lato name is reserved under OFL §3 and may not be used
on a Modified Version). Used by the Rust unit tests and the Python
integration tests for `@font-face`.

Lato is © 2010-2011 Łukasz Dziedzic, licensed under the SIL Open Font
License 1.1. The license requires Modified Versions to be re-licensed under
OFL — by including this subset we are redistributing it under OFL too. See
`OFL.txt` alongside this file for the full license text.

To regenerate the fixture (requires `fontTools` and a system Lato install):

```bash
uv run --with fonttools python3 tools/build_font_fixture.py
```
