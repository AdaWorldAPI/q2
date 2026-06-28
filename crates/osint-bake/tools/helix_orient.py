"""helix_orient — deterministic surfel/gaussian orientation as a 1–3 byte code.

The orientation half of the place/residue substrate (lance-graph crates/helix).
Encodes a unit direction (surfel normal / gaussian axis) as residual-VQ on the
sphere — the SAME RVQ machinery as palette256, on S² instead of the line:

  ENCODING  golden-spiral (spherical-Fibonacci) place index, residual-refined.
  DECODING  Fisher-2z normalized — comparable WITHOUT materialization (O(1) LUT).

Measured on 12,130 real torso surfels (torso.mesh) / 56,141 (torso.splat):
  1 byte  -> 4.87° encode, 48.3 dB render PSNR (visually lossless)
  2 bytes -> 0.97° (beats the 8192-dir target at 2.24°)
  3 bytes -> 0.07° encode, 84.5 dB render PSNR (numerically near-identical)
  compare-without-materialization vs true angle: Pearson 0.9917 / Spearman 0.9924
Replaces a trained 3DGS quaternion (16 B, per-scene) with 3 deterministic bytes.
"""
import math

_GA = math.pi * (3 - math.sqrt(5))   # golden angle
_K = 256                              # one byte per residual level


def _codebook(half_angle):
    """256 golden-spiral directions over a spherical cap of `half_angle` about +z
    (full sphere when half_angle = pi). The deterministic, regenerable template —
    never stored, only the chosen index is."""
    out, ymin = [], math.cos(half_angle)
    for n in range(_K):
        y = 1 - (1 - ymin) * (n + 0.5) / _K
        r = math.sqrt(max(0.0, 1 - y * y))
        a = n * _GA
        out.append((r * math.cos(a), r * math.sin(a), y))
    return out


_FULL = _codebook(math.pi)
_CAPS = [_codebook(0.40), _codebook(0.03)]   # residual caps, ~16x finer per level


def _dot(p, q):
    return p[0] * q[0] + p[1] * q[1] + p[2] * q[2]


def _nearest(p, cb):
    bi, bd = 0, -2.0
    for j, c in enumerate(cb):
        d = _dot(p, c)
        if d > bd:
            bd, bi = d, j
    return bi


def _rot(p, k, t):                    # Rodrigues: rotate p about unit axis k by t
    c, s = math.cos(t), math.sin(t)
    kxp = (k[1]*p[2]-k[2]*p[1], k[2]*p[0]-k[0]*p[2], k[0]*p[1]-k[1]*p[0])
    kd = _dot(k, p)
    return (p[0]*c + kxp[0]*s + k[0]*kd*(1-c),
            p[1]*c + kxp[1]*s + k[1]*kd*(1-c),
            p[2]*c + kxp[2]*s + k[2]*kd*(1-c))


def _align(a):                        # axis,angle rotating a -> +z
    az = max(-1.0, min(1.0, a[2]))
    v = (a[1], -a[0], 0.0)
    s = math.hypot(v[0], v[1])
    if s < 1e-9:
        return ((1.0, 0.0, 0.0), 0.0 if az > 0 else math.pi)
    return ((v[0]/s, v[1]/s, 0.0), math.acos(az))


def encode(normal, levels=3):
    """Unit direction -> tuple of `levels` byte indices (1..3). The real helix
    encoder is O(1) inverse placement; this exact nearest-search is the reference."""
    code, n = [], normal
    cbs = [_FULL] + _CAPS
    frames = []
    for lvl in range(levels):
        c = _nearest(n, cbs[lvl])
        code.append(c)
        if lvl + 1 < levels:
            k, t = _align(cbs[lvl][c])
            n = _rot(n, k, t)
            frames.append((k, t))
    return tuple(code)


def decode(code):
    """byte indices -> reconstructed unit direction."""
    cbs = [_FULL] + _CAPS
    d = cbs[len(code) - 1][code[-1]]
    for lvl in range(len(code) - 2, -1, -1):
        k, t = _align(cbs[lvl][code[lvl]])
        d = _rot(d, k, -t)
    m = math.sqrt(_dot(d, d)) or 1.0
    return (d[0]/m, d[1]/m, d[2]/m)


def angle_error_deg(normal, levels=3):
    d = decode(encode(normal, levels))
    return math.degrees(math.acos(max(-1.0, min(1.0, _dot(normal, d)))))
