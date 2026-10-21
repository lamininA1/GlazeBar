# Readable comment colors

GlazeBar exports palette slot 8 as VS Code's
`editor.tokenColorCustomizations.comments`. Previously, a contrast ratio below
4.5:1 against the background replaced this color with slot 15 (the body text
color). This could make Typst `//` comments look like ordinary text even when
Tinymist correctly identified them as comments.

Low-contrast slot 8 colors now keep their OKLCH hue and chroma while selecting
the nearest lightness, in 0.001 steps, whose exported RGB color meets 4.5:1.
Already readable colors are unchanged. RGB gamut clipping can slightly alter
the hue of saturated colors. Black or white provides a fallback if the search
cannot produce a readable color.

Slot 8 is shared by TextMate comments, terminal bright black, line numbers and
selection colors, so those exports receive the same correction. Other palette
slots retain their existing behavior. This does not change Tinymist or force
VS Code semantic-highlighting settings.

After installing the new build, select the wallpaper/theme again and apply it
to regenerate the palette and VS Code settings. Previously exported settings
are not rewritten merely by building or starting GlazeBar. Reusing an already
corrected old palette cannot recover its original muted color; regenerate it
from the wallpaper instead. The final-palette cache key is versioned so that
selecting an existing wallpaper regenerates old corrected palettes automatically.
Raw extraction caches and thumbnails remain reusable.

Regression tests cover dark and light backgrounds, warm color preservation,
contrast after RGB quantization, repeated application, VS Code settings, and
all 138 historical export cases. Historical Go extraction fixtures still
check all unchanged slots; the changed muted slot is checked for readability.
