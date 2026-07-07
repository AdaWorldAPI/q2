#!/usr/bin/env python3
"""Garmin IMG prototype decoder — proves the format understanding before the Rust port.

Container FAT -> subfile slices; TRE -> map levels + subdivision tree; RGN ->
points / polylines / polygons with the delta bitstream. Validated visually:
the decoded Grand Canyon tile must LOOK like the canyon.
"""
import struct, sys

def read_img(path):
    b = open(path, 'rb').read()
    xor = b[0]
    if xor:
        b = bytes(x ^ xor for x in b)
    assert b[0x10:0x16] == b'DSKIMG', "not a Garmin IMG"
    e1, e2 = b[0x61], b[0x62]
    blocksize = 1 << (e1 + e2)
    # FAT: 512-byte entries from 0x600 (skip the header pseudo-entry ' '*8).
    subs = {}
    o = 0x600
    while o + 512 <= len(b):
        flag = b[o]
        name = b[o+1:o+9].decode('ascii', 'replace')
        typ = b[o+9:o+12].decode('ascii', 'replace')
        if flag != 0x01:
            if flag == 0x00:
                break
            o += 512
            continue
        if name.strip() == '' or name[0] in ' \x00':
            o += 512
            continue
        size = struct.unpack('<I', b[o+12:o+16])[0]
        part = struct.unpack('<H', b[o+16:o+18])[0]
        key = (name.strip(), typ.strip())
        ent = subs.setdefault(key, {'size': 0, 'blocks': {}})
        if part == 0 and size:
            ent['size'] = size
        # 240 block pointers per FAT entry; part N covers blocks [N*240, ...)
        for i in range(240):
            blk = struct.unpack('<H', b[o+0x20+i*2:o+0x22+i*2])[0]
            if blk != 0xFFFF:
                ent['blocks'][part * 240 + i] = blk
        o += 512
    out = {}
    for (name, typ), ent in subs.items():
        idxs = sorted(ent['blocks'])
        data = b''.join(b[ent['blocks'][i]*blocksize:(ent['blocks'][i]+1)*blocksize] for i in idxs)
        size = ent['size'] or len(data)
        out[f"{name}.{typ}"] = data[:size]
    return out

def mu2deg(v, signed=True):
    if signed and v >= 1 << 23:
        v -= 1 << 24
    return v * 360.0 / (1 << 24)

def i24(b, o, signed=True):
    v = b[o] | b[o+1] << 8 | b[o+2] << 16
    if signed and v >= 1 << 23:
        v -= 1 << 24
    return v

def u16(b, o): return struct.unpack('<H', b[o:o+2])[0]
def u32(b, o): return struct.unpack('<I', b[o:o+4])[0]

class Level:
    __slots__ = ('zoom', 'inherited', 'bits', 'nsubdiv')

class Subdiv:
    __slots__ = ('n', 'rgn_off', 'objtypes', 'clon', 'clat', 'width', 'height',
                 'terminate', 'next', 'level', 'children', 'rgn_end')

def parse_tre(tre):
    hlen = u16(tre, 0)
    north, east = i24(tre, 0x15), i24(tre, 0x18)
    south, west = i24(tre, 0x1B), i24(tre, 0x1E)
    lvl_off, lvl_size = u32(tre, 0x21), u32(tre, 0x25)
    sub_off, sub_size = u32(tre, 0x29), u32(tre, 0x2D)
    levels = []
    for o in range(lvl_off, lvl_off + lvl_size, 4):
        L = Level()
        L.zoom = tre[o] & 0x7F
        L.inherited = bool(tre[o] & 0x80)
        L.bits = tre[o+1]
        L.nsubdiv = u16(tre, o+2)
        levels.append(L)
    # Subdivisions: all levels but the last use 16-byte records (with `next`);
    # the last (most detailed) level uses 14-byte records.
    subdivs = []
    o = sub_off
    n = 1  # subdivision indices are 1-based
    for li, L in enumerate(levels):
        last = li == len(levels) - 1
        rec = 14 if last else 16
        for _ in range(L.nsubdiv):
            if o + rec > sub_off + sub_size:
                break
            S = Subdiv()
            S.n = n
            S.rgn_off = i24(tre, o, signed=False) & 0x0FFFFFFF
            S.objtypes = tre[o+3]
            S.clon = i24(tre, o+4)
            S.clat = i24(tre, o+7)
            wraw = u16(tre, o+10)
            S.terminate = bool(wraw & 0x8000)
            S.width = wraw & 0x7FFF
            S.height = u16(tre, o+12) & 0x7FFF
            S.next = u16(tre, o+14) if not last else 0
            S.level = li
            S.children = []
            subdivs.append(S)
            o += rec
            n += 1
    return {'bbox': (north, east, south, west), 'levels': levels, 'subdivs': subdivs}

if __name__ == '__main__':
    path = sys.argv[1] if len(sys.argv) > 1 else '/home/user/q2/.claude/maps/garmin-grand-canyon/47505316.img'
    subs = read_img(path)
    print("subfiles:", {k: len(v) for k, v in subs.items()})
    tre_key = next(k for k in subs if k.endswith('.TRE'))
    tre = subs[tre_key]
    assert tre[2:12] == b'GARMIN TRE', tre[:16]
    T = parse_tre(tre)
    n, e, s, w = T['bbox']
    print(f"bbox N{mu2deg(n):.4f} E{mu2deg(e):.4f} S{mu2deg(s):.4f} W{mu2deg(w):.4f}")
    for i, L in enumerate(T['levels']):
        print(f"level {i}: zoom={L.zoom} bits={L.bits} subdivs={L.nsubdiv} inherited={L.inherited}")
    print(f"total subdivs parsed: {len(T['subdivs'])}")
    for S in T['subdivs'][:6]:
        print(f"  sd{S.n} L{S.level} rgn@{S.rgn_off} obj=0x{S.objtypes:02x} "
              f"c=({mu2deg(S.clon):.4f},{mu2deg(S.clat):.4f}) wh=({S.width},{S.height}) "
              f"term={S.terminate} next={S.next}")

# ── RGN decode ──────────────────────────────────────────────────────────────

class BitReader:
    def __init__(self, data):
        self.d = data
        self.pos = 0  # bit position
    def take(self, n):
        v = 0
        for i in range(n):
            byte = self.d[self.pos >> 3]
            bit = (byte >> (self.pos & 7)) & 1
            v |= bit << i
            self.pos += 1
        return v
    def remaining(self):
        return len(self.d) * 8 - self.pos

def base_bits(base):
    # imgformat: base 0-9 -> base+2 bits; base >9 -> 2*base-9 bits
    return base + 2 if base <= 9 else 2 * base - 9

def decode_bitstream(data, lon_base, lat_base, extra_bit):
    """Delta bitstream -> [(dlon, dlat)] in level units.

    Exact mirror of QMapShack CGarminPolygon/CShiftReg (the authoritative
    open-source decoder): per-axis sign info up front (same-sign flag; if set,
    one more bit = constant sign), per-delta two's complement otherwise, and —
    the crucial part — the CONTINUATION marker: in signed mode a raw value of
    1<<(n-1) (sign bit only) accumulates (2^(n-1) - 1) and reads on; the final
    value closes the sum (negative: tmp - 2^n - acc).
    """
    class _Out(Exception):
        pass
    br = BitReader(data)
    def take(n):
        if br.remaining() < n:
            raise _Out()
        return br.take(n)
    out = []
    try:
        def axis_info():
            if take(1):                      # same-sign mode
                return (False, -1 if take(1) else 1)
            return (True, 0)                 # per-delta signed
        x_has_sign, x_const = axis_info()
        y_has_sign, y_const = axis_info()
        nx = base_bits(lon_base) + (1 if x_has_sign else 0)
        ny = base_bits(lat_base) + (1 if y_has_sign else 0)
        xsign, ysign = 1 << (nx - 1), 1 << (ny - 1)
        while True:
            if extra_bit:
                take(1)                      # routing node flag
            if x_has_sign:
                acc = 0
                while True:
                    tmp = take(nx)
                    if tmp != xsign:
                        break
                    acc += tmp - 1           # continuation: += 2^(n-1) - 1
                dlon = acc + tmp if tmp < xsign else (tmp - (xsign << 1)) - acc
            else:
                dlon = take(nx) * x_const
            if y_has_sign:
                acc = 0
                while True:
                    tmp = take(ny)
                    if tmp != ysign:
                        break
                    acc += tmp - 1
                dlat = acc + tmp if tmp < ysign else (tmp - (ysign << 1)) - acc
            else:
                dlat = take(ny) * y_const
            out.append((dlon, dlat))
    except _Out:
        pass
    return out

def parse_rgn(rgn, tre_info, want_levels=None):
    """Yield dicts: {kind, type, level, coords[(lon,lat) mapunits], lbl}."""
    data_off = u32(rgn, 0x15)
    data_len = u32(rgn, 0x19)
    levels = tre_info['levels']
    # subdivisions that own RGN data, in file order (their rgn_off is ascending)
    live = [S for S in tre_info['subdivs'] if S.objtypes]
    live.sort(key=lambda S: S.rgn_off)
    for i, S in enumerate(live):
        S.rgn_end = live[i+1].rgn_off if i + 1 < len(live) else data_len
    feats = []
    for S in live:
        if want_levels is not None and S.level not in want_levels:
            continue
        bits = levels[S.level].bits
        shift = 24 - bits
        block = rgn[data_off + S.rgn_off: data_off + S.rgn_end]
        if not block:
            continue
        kinds = []           # (kind, mask) present, canonical order
        for kind, mask in (('point', 0x10), ('ipoint', 0x20), ('line', 0x40), ('poly', 0x80)):
            if S.objtypes & mask:
                kinds.append(kind)
        nptr = len(kinds) - 1
        ptrs = [u16(block, 2*i) for i in range(nptr)]
        starts = [2 * nptr] + ptrs
        ends = ptrs + [len(block)]
        for kind, s, e in zip(kinds, starts, ends):
            seg = block[s:e]
            o = 0
            if kind in ('point', 'ipoint'):
                while o + 9 <= len(seg):
                    t = seg[o]
                    lbl = i24(seg, o+1, signed=False)
                    has_sub = bool(lbl & 0x800000)
                    dlon = struct.unpack('<h', seg[o+4:o+6])[0]
                    dlat = struct.unpack('<h', seg[o+6:o+8])[0]
                    o += 9 if has_sub else 8
                    lon = S.clon + (dlon << shift)
                    lat = S.clat + (dlat << shift)
                    feats.append({'kind': 'point', 'type': t, 'level': S.level,
                                  'coords': [(lon, lat)], 'lbl': lbl & 0x3FFFFF})
            else:
                while o + 10 <= len(seg):
                    b0 = seg[o]
                    if kind == 'line':
                        t = b0 & 0x3F
                        two = bool(b0 & 0x80)
                    else:
                        t = b0 & 0x7F
                        two = bool(b0 & 0x80)
                    lblraw = i24(seg, o+1, signed=False)
                    extra = bool(lblraw & 0x400000) if kind == 'line' else False
                    lbl = lblraw & 0x3FFFFF
                    dlon = struct.unpack('<h', seg[o+4:o+6])[0]
                    dlat = struct.unpack('<h', seg[o+6:o+8])[0]
                    if two:
                        blen = u16(seg, o+8)
                        o2 = o + 10
                    else:
                        blen = seg[o+8]
                        o2 = o + 9
                    info = seg[o2]
                    lon_base = info & 0x0F
                    lat_base = info >> 4
                    bs = seg[o2+1: o2+1+blen]  # blen = bitstream bytes (info byte separate)
                    o = o2 + 1 + blen
                    lon = S.clon + (dlon << shift)
                    lat = S.clat + (dlat << shift)
                    coords = [(lon, lat)]
                    for dlo, dla in decode_bitstream(bs, lon_base, lat_base, extra):
                        lon += dlo << shift
                        lat += dla << shift
                        coords.append((lon, lat))
                    feats.append({'kind': kind, 'type': t, 'level': S.level,
                                  'coords': coords, 'lbl': lbl})
    return feats

# ── LBL decode (6-bit) — enough for numeric contour labels ─────────────────
T6 = " ABCDEFGHIJKLMNOPQRSTUVWXYZ~~~~~0123456789~~~~~~"
def parse_lbl(lbl):
    off = u32(lbl, 0x15)
    ln = u32(lbl, 0x19)
    mult = 1 << lbl[0x1D]
    enc = lbl[0x1E]
    return {'off': off, 'len': ln, 'mult': mult, 'enc': enc, 'data': lbl}

def label_text(L, lbloff):
    if lbloff == 0:
        return ''
    o = L['off'] + lbloff * L['mult']
    d = L['data']
    if L['enc'] == 9:  # 8-bit
        end = d.index(0, o)
        return d[o:end].decode('latin1', 'replace')
    out = []
    while o + 3 <= len(d):
        b0, b1, b2 = d[o], d[o+1], d[o+2]
        for c in (b0 >> 2, ((b0 & 3) << 4) | (b1 >> 4), ((b1 & 0xF) << 2) | (b2 >> 6), b2 & 0x3F):
            if c > 0x2F:
                return ''.join(out)
            out.append(T6[c] if c < len(T6) else '?')
        o += 3
    return ''.join(out)
