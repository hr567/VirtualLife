"""Font-generation regressions; standard-library only, no source download needed."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('bitmap_font', ROOT / 'tools/export-bitmap-font.py')
FONT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FONT)


class BitmapFontTests(unittest.TestCase):
    def test_every_shrinking_axis_source_point_has_one_exact_destination(self):
        for source_size in range(2, 19):
            for destination_size in range(1, source_size):
                for index in range(source_size):
                    destinations = [target for target in range(destination_size)
                                    if index in FONT.samples(target, source_size, destination_size)]
                    self.assertEqual(destinations, [index * destination_size // source_size])

    def test_each_isolated_dot_survives_two_axis_shrinking_without_spreading(self):
        source_width, source_height = 5, 7
        for width in range(1, source_width):
            for height in range(1, source_height):
                for y in range(source_height):
                    for x in range(source_width):
                        source = bytearray(source_width * source_height)
                        source[y * source_width + x] = 1
                        actual = FONT.resize_mask(source, source_width, source_height, width, height)
                        expected = bytearray(width * height)
                        expected[(y * height // source_height) * width + x * width // source_width] = 1
                        self.assertEqual(actual, expected)

    def test_enlarging_uses_nearest_neighbor_without_foreground_dilation(self):
        source = bytes((1, 0, 1, 0, 1, 0))
        expected = bytes(source[(y * 2 // 5) * 3 + x * 3 // 7]
                         for y in range(5) for x in range(7))
        self.assertEqual(FONT.resize_mask(source, 3, 2, 7, 5), expected)

    def test_mixed_axis_resize_preserves_each_source_dot(self):
        for y in range(5):
            for x in range(2):
                source = bytearray(10)
                source[y * 2 + x] = 1
                expected = bytes(int(target_y == y * 3 // 5 and target_x * 2 // 5 == x)
                                 for target_y in range(3) for target_x in range(5))
                self.assertEqual(FONT.resize_mask(source, 2, 5, 5, 3), expected)

    def test_saved_strikes_keep_all_printable_ascii_visible(self):
        catalog = json.loads((ROOT / 'assets/catalog/glyphs.json').read_text(encoding='utf8'))
        digest = catalog['sha256']
        pixels = (ROOT / f'assets/game/objects/{digest[:2]}/{digest}.bin').read_bytes()
        indices = {char: index for index, char in enumerate(catalog['characters'])}
        for strike in catalog['strikes']:
            for code in range(33, 127):
                start = strike['offset'] + indices[chr(code)] * strike['bytesPerGlyph']
                glyph = pixels[start:start + strike['bytesPerGlyph']]
                with self.subTest(height=strike['height'], character=chr(code)):
                    self.assertTrue(any(glyph))


if __name__ == '__main__':
    unittest.main()
