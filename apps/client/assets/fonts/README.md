# Console font

`RobotoMono.ttf` is Roboto Mono from the [Google Fonts Roboto Mono project](https://github.com/googlefonts/RobotoMono), designed by Christian Robertson. The adjacent `OFL.txt` contains its SIL Open Font License 1.1 terms.

The console rasterizes the bundled font with `fontdue` at runtime into a bounded CPU glyph atlas. The supported glyph cache includes Latin and extended Latin, combining marks, general punctuation, and the replacement character. Other scripts, including RTL and CJK, are outside this UI scope.
