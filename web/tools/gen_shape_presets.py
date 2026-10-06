#!/usr/bin/env python3
"""Generate src/shapes/preset-data.ts from LibreOffice's OOXML preset geometries.

Source of truth: oox/source/drawingml/customshapes/oox-drawingml-cs-presets in a
LibreOffice checkout — the exact adjustment values, guide equations, path segments,
coordinates, text frame and HANDLES (with ranges) of every OOXML preset shape.
Re-run manually when the mapping below grows:
    python3 tools/gen_shape_presets.py ~/libreoffice/core-master
"""
import json, re, sys, pathlib

SRC = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else
                   pathlib.Path.home() / 'libreoffice/core-master')
PRESETS = SRC / 'oox/source/drawingml/customshapes/oox-drawingml-cs-presets'
OUT = pathlib.Path(__file__).resolve().parent.parent / 'src/shapes/preset-data.ts'

# Our catalogue kind → OOXML preset name. Kinds absent here keep the hand-drawn
# generator (lines/connectors: their arrowheads are stroke properties, not paths).
KIND_TO_PRESET = {
    'rect': 'rect', 'roundRect': 'roundRect', 'snipRect': 'snip1Rect',
    'snip2SameRect': 'snip2SameRect', 'snip2DiagRect': 'snip2DiagRect',
    'snipRoundRect': 'snipRoundRect', 'roundRect1': 'round1Rect',
    'round2SameRect': 'round2SameRect', 'round2DiagRect': 'round2DiagRect',
    'plaque': 'plaque', 'frame': 'frame', 'halfFrame': 'halfFrame',
    'ellipse': 'ellipse', 'triangle': 'triangle', 'rtTriangle': 'rtTriangle',
    'parallelogram': 'parallelogram', 'trapezoid': 'trapezoid', 'diamond': 'diamond',
    'pentagon': 'pentagon', 'hexagon': 'hexagon', 'heptagon': 'heptagon',
    'octagon': 'octagon', 'decagon': 'decagon', 'dodecagon': 'dodecagon',
    'pie': 'pie', 'chord': 'chord', 'teardrop': 'teardrop', 'corner': 'corner',
    'diagStripe': 'diagStripe', 'cross': 'plus', 'bevel': 'bevel',
    'cylinder': 'can', 'cube': 'cube', 'blockArc': 'blockArc',
    'foldedCorner': 'foldedCorner', 'heart': 'heart', 'lightning': 'lightningBolt',
    'sun': 'sun', 'moon': 'moon', 'cloud': 'cloud', 'smiley': 'smileyFace',
    'arc': 'arc', 'donut': 'donut', 'noSymbol': 'noSmoking',
    'leftBrace': 'leftBrace', 'rightBrace': 'rightBrace',
    'leftBracket': 'leftBracket', 'rightBracket': 'rightBracket',
    'doubleBrace': 'bracePair', 'doubleBracket': 'bracketPair',
    'arrow': 'rightArrow', 'arrowLeft': 'leftArrow', 'arrowUp': 'upArrow',
    'arrowDown': 'downArrow', 'arrowLeftRight': 'leftRightArrow',
    'arrowUpDown': 'upDownArrow', 'arrowQuad': 'quadArrow',
    'leftRightUpArrow': 'leftRightUpArrow', 'chevron': 'chevron',
    'pentagonArrow': 'homePlate', 'bentArrow': 'bentArrow',
    'bentUpArrow': 'bentUpArrow', 'uTurnArrow': 'uturnArrow',
    'curvedRightArrow': 'curvedRightArrow', 'curvedLeftArrow': 'curvedLeftArrow',
    'curvedUpArrow': 'curvedUpArrow', 'curvedDownArrow': 'curvedDownArrow',
    'stripedRightArrow': 'stripedRightArrow', 'notchedArrow': 'notchedRightArrow',
    'circularArrow': 'circularArrow',
    'rightArrowCallout': 'rightArrowCallout', 'leftArrowCallout': 'leftArrowCallout',
    'upArrowCallout': 'upArrowCallout', 'downArrowCallout': 'downArrowCallout',
    'mathPlus': 'mathPlus', 'mathMinus': 'mathMinus', 'mathMultiply': 'mathMultiply',
    'mathDivide': 'mathDivide', 'mathEqual': 'mathEqual', 'mathNotEqual': 'mathNotEqual',
    'plus': 'mathPlus',
    'flowProcess': 'flowChartProcess', 'flowAltProcess': 'flowChartAlternateProcess',
    'flowDecision': 'flowChartDecision', 'flowData': 'flowChartInputOutput',
    'flowPredefined': 'flowChartPredefinedProcess', 'flowInternal': 'flowChartInternalStorage',
    'flowDocument': 'flowChartDocument', 'flowMultidoc': 'flowChartMultidocument',
    'flowTerminator': 'flowChartTerminator', 'flowPreparation': 'flowChartPreparation',
    'flowManualInput': 'flowChartManualInput', 'flowManualOp': 'flowChartManualOperation',
    'flowConnector': 'flowChartConnector', 'flowCard': 'flowChartPunchedCard',
    'flowPunchedTape': 'flowChartPunchedTape', 'flowOr': 'flowChartOr',
    'flowSumming': 'flowChartSummingJunction', 'flowCollate': 'flowChartCollate',
    'flowSort': 'flowChartSort', 'flowExtract': 'flowChartExtract',
    'flowMerge': 'flowChartMerge', 'flowStored': 'flowChartOnlineStorage',
    'flowSequential': 'flowChartMagneticTape', 'flowMagneticDisk': 'flowChartMagneticDisk',
    'flowDirectAccess': 'flowChartMagneticDrum', 'flowDisplay': 'flowChartDisplay',
    'flowDelay': 'flowChartDelay', 'flowOffPage': 'flowChartOffpageConnector',
    'star4': 'star4', 'star': 'star5', 'star6': 'star6', 'star7': 'star7',
    'star8': 'star8', 'star10': 'star10', 'star12': 'star12', 'star16': 'star16',
    'star24': 'star24', 'star32': 'star32',
    'explosion1': 'irregularSeal1', 'explosion2': 'irregularSeal2',
    'ribbon': 'ribbon', 'ribbonDown': 'ribbon2', 'ribbonCurved': 'ellipseRibbon',
    'scrollH': 'horizontalScroll', 'scrollV': 'verticalScroll',
    'wave': 'wave', 'doubleWave': 'doubleWave',
    'callout': 'wedgeRectCallout',
    'calloutRect': 'wedgeRectCallout', 'calloutRoundRect': 'wedgeRoundRectCallout',
    'calloutOval': 'wedgeEllipseCallout', 'calloutCloud': 'cloudCallout',
    'lineCallout': 'borderCallout1', 'calloutLine2': 'borderCallout2',
    'calloutLineAccent': 'accentCallout1',
}

text = PRESETS.read_text(encoding='utf-8')
blocks = re.split(r'/\* (\w+) \*/', text)[1:]
raw = dict(zip(blocks[0::2], blocks[1::2]))

PAIR = re.compile(
    r'First = \(com\.sun\.star\.drawing\.EnhancedCustomShapeParameter\) '
    r'\{ Value = \(any\) \{ \((?:long|short|double)\) (-?[\d.]+) \}, Type = \(short\) (\d+) \}, '
    r'Second = \(com\.sun\.star\.drawing\.EnhancedCustomShapeParameter\) '
    r'\{ Value = \(any\) \{ \((?:long|short|double)\) (-?[\d.]+) \}, Type = \(short\) (\d+) \}')
PARAM = re.compile(
    r'\(com\.sun\.star\.drawing\.EnhancedCustomShapeParameter\) '
    r'\{ Value = \(any\) \{ \((?:long|short|double)\) (-?[\d.]+) \}, Type = \(short\) (\d+) \}')

def num(v):
    f = float(v)
    return int(f) if f.is_integer() else f

def section(body, name):
    """Text of one top-level property (from its label to the next label)."""
    m = re.search(rf'^{name}$', body, re.M)
    if not m: return ''
    rest = body[m.end():]
    n = re.search(r'^(AdjustmentValues|Equations|Handles|MirroredX|MirroredY|Path|Type|ViewBox)$', rest, re.M)
    return rest[:n.start()] if n else rest

def prop(body, name):
    """Value text of a named PropertyValue inside a Path/Handles block."""
    i = body.find(f'Name = "{name}"')
    if i < 0: return ''
    j = body.find('Name = "', i + 10)
    return body[i:j if j > 0 else len(body)]

used_types = set()
def pairs_of(txt):
    out = []
    for a, ta, b, tb in PAIR.findall(txt):
        used_types.update((int(ta), int(tb)))
        out.append([[int(ta), num(a)], [int(tb), num(b)]])
    return out

def parse(name):
    body = raw[name]
    p = {}
    # adjustments, in declared order
    adv = section(body, 'AdjustmentValues')
    p['a'] = [[m.group(2), num(m.group(1))] for m in re.finditer(
        r'Value = \(any\) \{ \((?:long|double)\) (-?[\d.]+) \}, State = [^,]*, Name = "(\w+)"', adv)]
    eqs = section(body, 'Equations')
    p['e'] = re.findall(r'"([^"]*)"', eqs)
    path = section(body, 'Path')
    svs = prop(path, 'SubViewSize')
    if svs:
        # One awt.Size per sub-path: its own coordinate space, scaled to the box.
        p['v'] = [[num(w), num(h)] for w, h in re.findall(r'Width = \(long\) (-?\d+), Height = \(long\) (-?\d+)', svs)]
    p['c'] = pairs_of(prop(path, 'Coordinates'))
    p['s'] = [[int(c), int(n)] for c, n in re.findall(r'Command = \(short\) (\d+), Count = \(short\) (-?\d+)', prop(path, 'Segments'))]
    tf = prop(path, 'TextFrames')
    tfp = pairs_of(tf)
    if len(tfp) >= 2: p['t'] = tfp[:2]
    # handles
    hs = []
    htxt = section(body, 'Handles')
    # split on handle boundaries: each handle is a { { Name = "Position" ... } } group
    for hbody in re.split(r'\}, \{ \{ (?=Name = "Position")', htxt):
        if 'Name = "Position"' not in hbody: continue
        h = {}
        pos = pairs_of(prop(hbody, 'Position'))
        if not pos: continue
        h['p'] = pos[0]
        for key, tag in [('RefX', 'rx'), ('RefY', 'ry'), ('RefAngle', 'ra'), ('RefR', 'rr')]:
            m = re.search(rf'Name = "{key}", Handle = \(long\) \d+, Value = \(any\) \{{ \(long\) (-?\d+) \}}', hbody)
            if m: h[tag] = int(m.group(1))
        for key, tag in [('RangeXMinimum', 'xm'), ('RangeXMaximum', 'xM'),
                         ('RangeYMinimum', 'ym'), ('RangeYMaximum', 'yM'),
                         ('RadiusRangeMinimum', 'rm'), ('RadiusRangeMaximum', 'rM')]:
            t = prop(hbody, key)
            mm = PARAM.search(t)
            if mm:
                used_types.add(int(mm.group(2)))
                h[tag] = [int(mm.group(2)), num(mm.group(1))]
        hs.append(h)
    if hs: p['h'] = hs
    return p

out, missing, subview = {}, [], []
for kind, preset in KIND_TO_PRESET.items():
    if preset not in raw:
        missing.append((kind, preset)); continue
    if preset not in out:
        out[preset] = parse(preset)
        if out[preset].get('v'): subview.append(preset)

header = (
    "// GENERATED by tools/gen_shape_presets.py — DO NOT EDIT BY HAND.\n"
    "// Exact OOXML preset geometries (adjustments, guide equations, path, text frame,\n"
    "// handles with ranges), extracted from LibreOffice's\n"
    "// oox/source/drawingml/customshapes/oox-drawingml-cs-presets.\n"
    "// Param = [type, value]; types: 0 constant, 1 equation index, 2 adjustment index.\n\n"
    "export type Param = [number, number]\n"
    "export interface PresetHandle {\n"
    "  p: [Param, Param]\n"
    "  rx?: number; ry?: number; ra?: number; rr?: number\n"
    "  xm?: Param; xM?: Param; ym?: Param; yM?: Param; rm?: Param; rM?: Param\n"
    "}\n"
    "export interface Preset {\n"
    "  a: [string, number][]\n"
    "  e: string[]\n"
    "  c: [Param, Param][]\n"
    "  s: [number, number][]\n"
    "  t?: [[Param, Param], [Param, Param]]\n"
    "  h?: PresetHandle[]\n"
    "  /** One [w, h] per sub-path: its own coordinate space, scaled to the box. */\n"
    "  v?: [number, number][]\n"
        "}\n\n"
)
kind_map = {k: v for k, v in KIND_TO_PRESET.items() if v in out}
body = (header
        + "export const PRESETS: Record<string, Preset> = "
        + json.dumps(out, separators=(',', ':')) + "\n\n"
        + "export const KIND_TO_PRESET: Record<string, string> = "
        + json.dumps(kind_map, separators=(',', ':')) + "\n")
OUT.write_text(body, encoding='utf-8')
print(f"presets extraits : {len(out)} · kinds mappés : {len(kind_map)}")
print(f"types de paramètres rencontrés : {sorted(used_types)}")
if missing: print("INTROUVABLES :", missing)
if subview: print("SubViewSize (espaces propres, gérés) :", len(subview))
# Rust twin table: kind ↔ prst + adjustment names, for a faithful avLst in xlsx.
RUST = OUT.parent.parent.parent.parent / 'src/converters/xlsx/shape_presets.rs'
lines = [
    "// GENERATED by frontend/tools/gen_shape_presets.py — DO NOT EDIT BY HAND.",
    "// Internal shape kind ↔ OOXML preset name, with each preset's adjustment",
    "// names in order — so the xlsx writer emits an exact <a:avLst> and the",
    "// reader maps one back, both agreeing with LibreOffice's preset data.",
    "",
    "/// (kind, prst, adjustment names in order)",
    "pub const KIND_PRESETS: &[(&str, &str, &[&str])] = &[",
]
for kind, preset in KIND_TO_PRESET.items():
    if preset not in out: continue
    names = ', '.join(f'"{n}"' for n, _ in out[preset]['a'])
    lines.append(f'    ("{kind}", "{preset}", &[{names}]),')
lines += ["];", ""]
RUST.write_text("\n".join(lines), encoding='utf-8')
print(f"table Rust : {RUST}")
print(f"taille : {OUT.stat().st_size // 1024} Ko → {OUT}")
