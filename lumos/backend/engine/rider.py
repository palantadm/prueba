"""Technical Rider PDF generator.

Produces a professional technical rider document containing:
  1. Cover page — show name, generation date, venue
  2. Equipment list — fixture inventory grouped by model
  3. DMX Patch — full channel patch table per universe
  4. Universe outputs — Art-Net / sACN / KiNET / serial config
  5. Cue lists — all cue lists with timings
  6. Effects — active effects and waveform params
  7. Groups & submasters
  8. Notes / requirements section
"""
import io
import time
from typing import Optional

try:
    from reportlab.lib import colors
    from reportlab.lib.enums import TA_CENTER, TA_LEFT, TA_RIGHT
    from reportlab.lib.pagesizes import A4, letter
    from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
    from reportlab.lib.units import mm, cm
    from reportlab.platypus import (
        BaseDocTemplate, Frame, PageTemplate,
        Paragraph, Spacer, Table, TableStyle, HRFlowable,
        KeepTogether, PageBreak, NextPageTemplate,
    )
    from reportlab.platypus.flowables import Flowable
    from reportlab.graphics.shapes import Drawing, Rect, String, Line, Circle
    from reportlab.graphics import renderPDF
    REPORTLAB_AVAILABLE = True
except ImportError:
    REPORTLAB_AVAILABLE = False


# ── Color palette (dark-show aesthetic) ───────────────────────────────────────

C_BG        = colors.HexColor('#0a0a1a')
C_HEADER    = colors.HexColor('#1a1a2e')
C_ACCENT    = colors.HexColor('#7c6fcd')
C_ACCENT_HI = colors.HexColor('#9d8ff0')
C_TEXT      = colors.HexColor('#e0e0f0')
C_TEXT_DIM  = colors.HexColor('#888890')
C_LINE      = colors.HexColor('#2e2e50')
C_ROW_ALT   = colors.HexColor('#12121e')
C_ROW_EVEN  = colors.HexColor('#0f0f1a')
C_GREEN     = colors.HexColor('#4caf78')
C_AMBER     = colors.HexColor('#f0a030')
C_RED       = colors.HexColor('#e05555')
C_CYAN      = colors.HexColor('#40b8d0')
C_WHITE     = colors.white
C_BLACK     = colors.black


def _style(name='Normal', **kw):
    """Quick ParagraphStyle factory."""
    base = getSampleStyleSheet()[name]
    return ParagraphStyle(name + '_custom', parent=base, **kw)


# ── Shared styles ──────────────────────────────────────────────────────────────

def _make_styles():
    return {
        'title':    _style('Heading1', fontSize=28, textColor=C_ACCENT_HI,
                           alignment=TA_CENTER, spaceAfter=4,
                           fontName='Helvetica-Bold'),
        'subtitle': _style('Normal',   fontSize=13, textColor=C_TEXT_DIM,
                           alignment=TA_CENTER, spaceAfter=2,
                           fontName='Helvetica'),
        'section':  _style('Heading2', fontSize=11, textColor=C_ACCENT_HI,
                           spaceBefore=14, spaceAfter=6,
                           fontName='Helvetica-Bold'),
        'body':     _style('Normal',   fontSize=8,  textColor=C_TEXT,
                           fontName='Helvetica', leading=11),
        'mono':     _style('Normal',   fontSize=7,  textColor=C_TEXT,
                           fontName='Courier', leading=10),
        'caption':  _style('Normal',   fontSize=7,  textColor=C_TEXT_DIM,
                           fontName='Helvetica', alignment=TA_CENTER),
        'th':       _style('Normal',   fontSize=7,  textColor=C_WHITE,
                           fontName='Helvetica-Bold', alignment=TA_CENTER),
        'td':       _style('Normal',   fontSize=7,  textColor=C_TEXT,
                           fontName='Helvetica'),
        'td_mono':  _style('Normal',   fontSize=7,  textColor=C_CYAN,
                           fontName='Courier'),
        'td_c':     _style('Normal',   fontSize=7,  textColor=C_TEXT,
                           fontName='Helvetica', alignment=TA_CENTER),
    }


# ── Table style helpers ────────────────────────────────────────────────────────

def _tbl_style(extra=None):
    base = [
        ('BACKGROUND',   (0, 0), (-1,  0), C_HEADER),
        ('TEXTCOLOR',    (0, 0), (-1,  0), C_WHITE),
        ('FONTNAME',     (0, 0), (-1,  0), 'Helvetica-Bold'),
        ('FONTSIZE',     (0, 0), (-1, -1), 7),
        ('ROWBACKGROUNDS',(0,1),(-1,-1), [C_ROW_EVEN, C_ROW_ALT]),
        ('TEXTCOLOR',    (0, 1), (-1, -1), C_TEXT),
        ('GRID',         (0, 0), (-1, -1), 0.4, C_LINE),
        ('TOPPADDING',   (0, 0), (-1, -1), 3),
        ('BOTTOMPADDING',(0, 0), (-1, -1), 3),
        ('LEFTPADDING',  (0, 0), (-1, -1), 5),
        ('RIGHTPADDING', (0, 0), (-1, -1), 5),
        ('VALIGN',       (0, 0), (-1, -1), 'MIDDLE'),
    ]
    if extra:
        base.extend(extra)
    return TableStyle(base)


# ── Page templates ─────────────────────────────────────────────────────────────

def _make_doc(buf, show_name: str, page_size=A4):
    W, H = page_size
    margin = 18 * mm

    def _header_footer(canvas, doc):
        canvas.saveState()
        # Top bar
        canvas.setFillColor(C_HEADER)
        canvas.rect(0, H - 14*mm, W, 14*mm, fill=1, stroke=0)
        canvas.setFont('Helvetica-Bold', 8)
        canvas.setFillColor(C_ACCENT_HI)
        canvas.drawString(margin, H - 9*mm, '⚡ LUMOS')
        canvas.setFont('Helvetica', 7)
        canvas.setFillColor(C_TEXT_DIM)
        canvas.drawCentredString(W/2, H - 9*mm, show_name.upper())
        canvas.drawRightString(W - margin, H - 9*mm, 'TECHNICAL RIDER')
        # Bottom bar
        canvas.setFillColor(C_HEADER)
        canvas.rect(0, 0, W, 10*mm, fill=1, stroke=0)
        canvas.setFont('Helvetica', 6.5)
        canvas.setFillColor(C_TEXT_DIM)
        canvas.drawString(margin, 4*mm, f'Generated by Lumos  ·  {time.strftime("%Y-%m-%d %H:%M")}')
        canvas.drawCentredString(W/2, 4*mm, f'Page {doc.page}')
        canvas.drawRightString(W - margin, 4*mm, 'CONFIDENTIAL — FOR CREW USE ONLY')
        canvas.restoreState()

    content_frame = Frame(
        margin, 12*mm,
        W - 2*margin, H - 14*mm - 12*mm,
        leftPadding=0, rightPadding=0, topPadding=4, bottomPadding=4,
    )
    cover_frame = Frame(0, 0, W, H, leftPadding=0, rightPadding=0,
                        topPadding=0, bottomPadding=0)

    doc = BaseDocTemplate(
        buf, pagesize=page_size,
        leftMargin=margin, rightMargin=margin,
        topMargin=16*mm, bottomMargin=12*mm,
    )
    doc.addPageTemplates([
        PageTemplate(id='cover',   frames=[cover_frame]),
        PageTemplate(id='content', frames=[content_frame], onPage=_header_footer),
    ])
    return doc


# ── Cover page ─────────────────────────────────────────────────────────────────

def _cover_page(show_name: str, venue: str, date_str: str, S: dict) -> list:
    from reportlab.platypus import Flowable as _F

    class CoverPage(_F):
        def __init__(self, show_name, venue, date_str):
            _F.__init__(self)
            self.show_name = show_name
            self.venue = venue
            self.date_str = date_str
            self.width = A4[0]
            self.height = A4[1]

        def wrap(self, *_):
            return self.width, self.height

        def draw(self):
            c = self.canv
            W, H = self.width, self.height

            # Full background
            c.setFillColor(C_BG)
            c.rect(0, 0, W, H, fill=1, stroke=0)

            # Accent stripe left
            c.setFillColor(C_ACCENT)
            c.rect(0, 0, 4*mm, H, fill=1, stroke=0)

            # Decorative grid lines
            c.setStrokeColor(C_LINE)
            c.setLineWidth(0.3)
            for y in range(0, int(H), 20):
                c.line(0, y, W, y)

            # Big glow circle
            c.setFillColor(colors.HexColor('#0f0a28'))
            c.circle(W/2, H*0.55, 90*mm, fill=1, stroke=0)
            c.setFillColor(colors.HexColor('#1a123a'))
            c.circle(W/2, H*0.55, 60*mm, fill=1, stroke=0)

            # Logo
            c.setFont('Helvetica-Bold', 48)
            c.setFillColor(C_ACCENT_HI)
            c.drawCentredString(W/2, H*0.62, '⚡')
            c.setFont('Helvetica-Bold', 36)
            c.setFillColor(C_ACCENT_HI)
            c.drawCentredString(W/2, H*0.55, 'LUMOS')
            c.setFont('Helvetica', 9)
            c.setFillColor(C_TEXT_DIM)
            c.drawCentredString(W/2, H*0.515, 'THEATRICAL LIGHTING CONTROL')

            # Horizontal rule
            c.setStrokeColor(C_ACCENT)
            c.setLineWidth(1)
            c.line(30*mm, H*0.47, W-30*mm, H*0.47)

            # Show name
            c.setFont('Helvetica-Bold', 22)
            c.setFillColor(C_WHITE)
            c.drawCentredString(W/2, H*0.40, self.show_name.upper())

            # Venue / date
            c.setFont('Helvetica', 10)
            c.setFillColor(C_TEXT_DIM)
            if self.venue:
                c.drawCentredString(W/2, H*0.365, self.venue)
            c.drawCentredString(W/2, H*0.34, self.date_str)

            # Document type tag
            c.setFillColor(C_ACCENT)
            c.roundRect(W/2-28*mm, H*0.29, 56*mm, 8*mm, 2*mm, fill=1, stroke=0)
            c.setFont('Helvetica-Bold', 8)
            c.setFillColor(C_WHITE)
            c.drawCentredString(W/2, H*0.295, 'TECHNICAL RIDER / LIGHTING SPECIFICATION')

            # Footer
            c.setFont('Helvetica', 7)
            c.setFillColor(C_TEXT_DIM)
            c.drawCentredString(W/2, 12*mm, f'Generated: {time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())}')
            c.drawCentredString(W/2, 8*mm, 'CONFIDENTIAL — FOR CREW USE ONLY')

    return [CoverPage(show_name, venue, date_str), NextPageTemplate('content'), PageBreak()]


# ── Section helpers ────────────────────────────────────────────────────────────

def _section(title: str, S: dict):
    return [
        Spacer(1, 2*mm),
        Paragraph(title, S['section']),
        HRFlowable(width='100%', thickness=0.5, color=C_ACCENT, spaceAfter=4),
    ]


def _no_data(msg: str, S: dict):
    return [Paragraph(f'<font color="#{C_TEXT_DIM.hexval()[2:]}"><i>{msg}</i></font>', S['body'])]


# ── Section 1: Equipment list ──────────────────────────────────────────────────

def _equipment_section(patch, S: dict) -> list:
    story = _section('1 — Equipment List', S)

    fixtures = list(patch.fixtures.values())
    if not fixtures:
        return story + _no_data('No fixtures patched.', S)

    # Group by definition
    from collections import Counter, defaultdict
    groups = defaultdict(list)
    for f in fixtures:
        key = f'{f.manufacturer} {f.model}' if hasattr(f, 'manufacturer') else f.name
        groups[key].append(f)

    # Summary table
    rows = [['#', 'Manufacturer', 'Model', 'Type', 'Mode', 'CH / Fixture', 'Qty']]
    for i, (key, lst) in enumerate(sorted(groups.items()), 1):
        f = lst[0]
        mfr   = getattr(f, 'manufacturer', '—')
        model = getattr(f, 'model', key)
        ftype = getattr(f, 'fixture_type', '—')
        mode  = getattr(f, 'mode', '—')
        ch    = str(len(getattr(f, 'channel_order', [])))
        rows.append([str(i), mfr, model, ftype, mode, ch, str(len(lst))])

    # Grand total row
    rows.append(['', '', '', '', '', 'TOTAL', str(len(fixtures))])
    col_w = [8*mm, 38*mm, 42*mm, 30*mm, 18*mm, 22*mm, 12*mm]
    tbl = Table(rows, colWidths=col_w, repeatRows=1)
    extra = [
        ('BACKGROUND', (0, len(rows)-1), (-1, len(rows)-1), C_ACCENT),
        ('TEXTCOLOR',  (0, len(rows)-1), (-1, len(rows)-1), C_WHITE),
        ('FONTNAME',   (0, len(rows)-1), (-1, len(rows)-1), 'Helvetica-Bold'),
        ('ALIGN',      (5, 1), (6, -1), 'CENTER'),
    ]
    tbl.setStyle(_tbl_style(extra))
    story.append(KeepTogether([tbl]))
    return story


# ── Section 2: DMX Patch ───────────────────────────────────────────────────────

def _patch_section(patch, universes, S: dict) -> list:
    story = _section('2 — DMX Patch', S)
    fixtures = sorted(patch.fixtures.values(), key=lambda f: (getattr(f, 'universe', 1), getattr(f, 'address', 1)))
    if not fixtures:
        return story + _no_data('No fixtures patched.', S)

    # Group by universe
    from collections import defaultdict
    by_univ = defaultdict(list)
    for f in fixtures:
        by_univ[getattr(f, 'universe', 1)].append(f)

    for uid, flist in sorted(by_univ.items()):
        uname = universes.universes.get(uid)
        univ_label = f'Universe {uid}' + (f' — {uname.name}' if uname else '')
        story.append(Paragraph(univ_label, S['body']))
        story.append(Spacer(1, 1*mm))

        rows = [['Addr', 'Name', 'Model', 'Mode', 'Channels', 'Last Ch', 'Group']]
        for f in flist:
            addr    = getattr(f, 'address', '?')
            name    = getattr(f, 'name', '?')
            model   = getattr(f, 'model', '?')
            mode    = getattr(f, 'mode', '?')
            ch_list = getattr(f, 'channel_order', [])
            n_ch    = len(ch_list)
            last_ch = addr + n_ch - 1 if n_ch else addr
            group   = getattr(f, 'group', '')
            rows.append([str(addr), name, model, mode, str(n_ch), str(last_ch), group])

        col_w = [12*mm, 42*mm, 36*mm, 14*mm, 16*mm, 14*mm, 22*mm]
        tbl = Table(rows, colWidths=col_w, repeatRows=1)
        tbl.setStyle(_tbl_style([('ALIGN', (0,1), (0,-1), 'CENTER'),
                                  ('FONTNAME', (0,1), (0,-1), 'Courier'),
                                  ('TEXTCOLOR', (0,1), (0,-1), C_AMBER)]))
        story.append(KeepTogether([tbl]))
        story.append(Spacer(1, 4*mm))

    return story


# ── Section 3: Universe output config ─────────────────────────────────────────

def _universe_section(universes, S: dict) -> list:
    story = _section('3 — Universe Output Configuration', S)
    if not universes.universes:
        return story + _no_data('No universes configured.', S)

    rows = [['Universe', 'Name', 'Output Type', 'Destination', 'Parameters']]
    for uid, u in sorted(universes.universes.items()):
        if not u.outputs:
            rows.append([str(uid), u.name, 'None', '—', '—'])
        for out in u.outputs:
            if out.type == 'artnet':
                dest   = out.artnet_ip
                params = f'Net:{out.artnet_net} Sub:{out.artnet_subnet} Univ:{out.artnet_universe}'
            elif out.type == 'sacn':
                dest   = out.sacn_dest_ip or 'Multicast'
                params = f'sACN Universe:{out.sacn_universe} Priority:{out.sacn_priority}'
            elif out.type == 'kinet':
                dest   = out.kinet_ip
                params = f'Port:{out.kinet_port} v{out.kinet_version}'
            elif out.type == 'serial':
                dest   = out.serial_port
                params = f'Mode:{out.serial_mode}'
            else:
                dest   = '—'
                params = '—'
            rows.append([str(uid), u.name, out.type.upper(), dest, params])

    col_w = [14*mm, 32*mm, 22*mm, 42*mm, 58*mm]
    tbl = Table(rows, colWidths=col_w, repeatRows=1)
    tbl.setStyle(_tbl_style())
    story.append(KeepTogether([tbl]))
    return story


# ── Section 4: Cue lists ───────────────────────────────────────────────────────

def _cue_section(cues, S: dict) -> list:
    story = _section('4 — Cue Lists', S)
    cue_lists = getattr(cues, 'cue_lists', {})
    if not cue_lists:
        return story + _no_data('No cue lists programmed.', S)

    for cl_id, cl in cue_lists.items():
        story.append(Paragraph(
            f'<b>{cl.name}</b>  <font color="#{C_TEXT_DIM.hexval()[2:]}">({len(cl.cues)} cues{" · Loop" if cl.loop else ""})</font>',
            S['body']
        ))
        story.append(Spacer(1, 1*mm))
        rows = [['#', 'ID', 'Name', 'Fade In', 'Hold', 'Fade Type', 'Fixtures']]
        for i, cue in enumerate(cl.cues, 1):
            n_fixtures = len(cue.channels) if hasattr(cue, 'channels') else 0
            rows.append([
                str(i),
                cue.id[:8] + '…' if len(cue.id) > 8 else cue.id,
                cue.name,
                f'{cue.fade_in:.1f}s',
                f'{cue.hold:.1f}s' if cue.hold else '—',
                getattr(cue, 'fade_type', 'linear'),
                str(n_fixtures),
            ])
        col_w = [8*mm, 20*mm, 50*mm, 16*mm, 14*mm, 18*mm, 18*mm]
        tbl = Table(rows, colWidths=col_w, repeatRows=1)
        tbl.setStyle(_tbl_style([
            ('TEXTCOLOR', (3,1), (4,-1), C_AMBER),
            ('FONTNAME',  (3,1), (4,-1), 'Courier'),
        ]))
        story.append(KeepTogether([tbl]))
        story.append(Spacer(1, 5*mm))

    return story


# ── Section 5: Effects ────────────────────────────────────────────────────────

def _effects_section(effects, S: dict) -> list:
    story = _section('5 — Programmed Effects', S)
    efx = getattr(effects, 'effects', {})
    if not efx:
        return story + _no_data('No effects programmed.', S)

    rows = [['Name', 'Waveform', 'Speed (Hz)', 'BPM Mult.', 'Amplitude', 'Center', 'Channels', 'Enabled']]
    for eff in efx.values():
        bpm = f'×{eff.bpm_multiplier}' if eff.bpm_sync else '—'
        rows.append([
            eff.name,
            eff.waveform.value if hasattr(eff.waveform, 'value') else str(eff.waveform),
            f'{eff.speed:.2f}',
            bpm,
            str(eff.amplitude),
            str(eff.center),
            str(len(eff.channels)),
            '✓' if eff.enabled else '✗',
        ])

    col_w = [38*mm, 22*mm, 18*mm, 18*mm, 18*mm, 16*mm, 16*mm, 14*mm]
    tbl = Table(rows, colWidths=col_w, repeatRows=1)
    tbl.setStyle(_tbl_style([
        ('TEXTCOLOR', (2,1), (3,-1), C_CYAN),
        ('ALIGN',     (7,0), (7,-1), 'CENTER'),
        ('TEXTCOLOR', (7,1), (7,-1), C_GREEN),
    ]))
    story.append(KeepTogether([tbl]))
    return story


# ── Section 6: Groups & submasters ────────────────────────────────────────────

def _groups_section(groups, patch, S: dict) -> list:
    story = _section('6 — Groups & Submasters', S)
    grp_list = getattr(groups, 'groups', {})
    if not grp_list:
        return story + _no_data('No groups defined.', S)

    rows = [['Group Name', 'Fixtures', 'Submaster %', 'Grand Master %']]
    gm_pct = f'{int(groups.grand_master * 100)}%' if hasattr(groups, 'grand_master') else '100%'
    for grp in grp_list.values():
        sm_pct = f'{int(grp.submaster * 100)}%' if hasattr(grp, 'submaster') else '100%'
        n_fix  = len(grp.fixture_ids) if hasattr(grp, 'fixture_ids') else 0
        rows.append([grp.name, str(n_fix), sm_pct, gm_pct])

    col_w = [50*mm, 20*mm, 30*mm, 30*mm]
    tbl = Table(rows, colWidths=col_w, repeatRows=1)
    tbl.setStyle(_tbl_style([('ALIGN', (1,0), (-1,-1), 'CENTER')]))
    story.append(KeepTogether([tbl]))

    # Grand master note
    story.append(Spacer(1, 3*mm))
    story.append(Paragraph(
        f'Grand Master level at save time: <b>{gm_pct}</b>  '
        f'Blackout active: <b>{"YES" if getattr(groups, "blackout", False) else "no"}</b>',
        S['body']
    ))
    return story


# ── Section 7: Notes ──────────────────────────────────────────────────────────

def _notes_section(notes: str, S: dict) -> list:
    story = _section('7 — Production Notes', S)
    story.append(Paragraph(notes or 'No notes provided.', S['body']))
    # Blank lines for handwritten annotations
    for _ in range(6):
        story.append(Spacer(1, 5*mm))
        story.append(HRFlowable(width='100%', thickness=0.3, color=C_LINE))
    return story


# ── Main entry point ──────────────────────────────────────────────────────────

def generate_rider_pdf(
    patch,
    universes,
    cues,
    effects,
    groups,
    show_name: str = 'Show',
    venue: str = '',
    date_str: str = '',
    notes: str = '',
    page_size=None,
) -> bytes:
    """Build the PDF and return it as bytes."""
    if not REPORTLAB_AVAILABLE:
        raise ImportError('reportlab is required: pip install reportlab')

    if page_size is None:
        page_size = A4
    if not date_str:
        date_str = time.strftime('%Y-%m-%d')

    buf = io.BytesIO()
    doc = _make_doc(buf, show_name, page_size)
    S = _make_styles()

    story = []
    story += _cover_page(show_name, venue, date_str, S)
    story += _equipment_section(patch, S)
    story += _patch_section(patch, universes, S)
    story += _universe_section(universes, S)
    story += _cue_section(cues, S)
    story += _effects_section(effects, S)
    story += _groups_section(groups, patch, S)
    story += _notes_section(notes, S)

    doc.build(story)
    return buf.getvalue()
