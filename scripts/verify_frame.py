#!/usr/bin/env python3
"""Verify the rendered frame actually contains a triangle by inspecting raw pixels."""
import struct

with open("/tmp/frame.raw", "rb") as f:
    data = f.read()

W, H = 800, 600
assert len(data) == W * H * 4, f"bad size {len(data)}"

# Background is supposed to be (0.05, 0.05, 0.08) BGRA = (B=20, G=13, R=13, A=255)
# Triangle vertices: red, green, blue — at:
#   red:    (0.5, -0.5) — bottom-right of NDC = pixel (600, 450)
#   green: (-0.5, -0.5) — bottom-left  = pixel (200, 450)
#   blue:   (0.0,  0.5) — top-middle    = pixel (400, 150)

def pixel_at(x, y):
    off = (y * W + x) * 4
    b, g, r, a = data[off], data[off+1], data[off+2], data[off+3]
    return (r, g, b, a)

print("background (corner 0,0):", pixel_at(0, 0))
print("inside triangle (400, 300):", pixel_at(400, 300))
print("near red vertex (600, 450):", pixel_at(600, 450))
print("near green vertex (200, 450):", pixel_at(200, 450))
print("near blue vertex (400, 150):", pixel_at(400, 150))

# Sample a horizontal scan to find colored pixels
distinct_colors = set()
for y in range(0, H, 6):
    for x in range(0, W, 6):
        c = pixel_at(x, y)
        if c[3] == 255 and (abs(c[0] - 13) > 5 or abs(c[1] - 13) > 5 or abs(c[2] - 20) > 5):
            # not background
            distinct_colors.add(c)
print(f"distinct non-background colors found: {len(distinct_colors)}")
for c in list(distinct_colors)[:5]:
    print("  ", c)
