<#
.SYNOPSIS
Regenerates Windows icons with alpha and legacy transparency masks.
#>
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    @'
from io import BytesIO
from pathlib import Path
import struct
from PIL import Image

source = Image.open('src-tauri/app-icon.png').convert('RGBA')
icons = Path('src-tauri/icons')
sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
frames = []
for size in sizes:
    frame = source.resize((size, size), Image.Resampling.LANCZOS)
    if size == 256:
        png = BytesIO()
        frame.save(png, format='PNG')
        data = png.getvalue()
    else:
        # Small Windows shell icons can use the AND mask instead of alpha.
        # Include both, with transparent RGB cleared to avoid coloured fringes.
        pixels = bytearray()
        stride = ((size + 31) // 32) * 4
        mask = bytearray(stride * size)
        for y in range(size - 1, -1, -1):
            for x in range(size):
                r, g, b, a = frame.getpixel((x, y))
                if a == 0:
                    r = g = b = 0
                pixels.extend((b, g, r, a))
                if a < 128:
                    mask[(size - 1 - y) * stride + x // 8] |= 0x80 >> (x % 8)
        data = struct.pack('<IiiHHIIiiII', 40, size, size * 2, 1, 32,
                           0, len(pixels), 0, 0, 0, 0) + pixels + mask
    frames.append((size, data))
offset = 6 + 16 * len(frames)
directory = bytearray(struct.pack('<HHH', 0, 1, len(frames)))
for size, data in frames:
    directory.extend(struct.pack('<BBBBHHII', size % 256, size % 256,
                                 0, 0, 1, 32, len(data), offset))
    offset += len(data)
(icons / 'icon.ico').write_bytes(directory + b''.join(data for _, data in frames))
for size, name in [(32, '32x32.png'), (64, '64x64.png'),
                   (128, '128x128.png'), (256, '128x128@2x.png')]:
    source.resize((size, size), Image.Resampling.LANCZOS).save(icons / name)
print('Generated Windows icons (Pillow required).')
'@ | python -
    if ($LASTEXITCODE -ne 0) { throw 'Icon generation failed' }
}
finally { Pop-Location }
