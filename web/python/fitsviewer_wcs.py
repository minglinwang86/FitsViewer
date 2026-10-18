"""Validated, in-memory WCS overlays; never rewrite image headers or pixels.

校准只替换内存中的坐标模型，保留原始数据、尺寸、单位和定标字段。
"""
import re

_WCS_KEY = re.compile(r'^(WCSAXES|WCSNAME|RADESYS|RADECSYS|EQUINOX|LONPOLE|LATPOLE|MJDREF|'
                      r'(CTYPE|CUNIT|CRVAL|CRPIX|CDELT|CROTA)[12]|(CD|PC)[12]_[12]|'
                      r'(PV|PS)[12]_\d+|(A|B|AP|BP)_ORDER|(A|B|AP|BP)_\d+_\d+|'
                      r'CQDIS[12]|DQ[12]\.[A-Z0-9._-]+)$')
# WCSLIB prioritizes a legacy DSS plate model if these cards survive a TAN overlay.
# 替换 Gaia 坐标模型时清除旧 DSS 底片模型，避免旧解优先于新的 TAN 模型。
_DSS_KEY = re.compile(r'^(PLTRA[HMS]|PLTDEC(SN|[DMS])|PLTSCALE|PLTEQUIN|'
                      r'PPO[1-6]|AMD[XY]\d+|CNPIX[12]|[XY]PIXELSZ)$')


def validate_override(text):
    if text is not None and (not isinstance(text, str) or not text.strip() or len(text.encode('utf-8')) > 65536):
        raise ValueError('WCS override must be a nonempty FITS header of at most 64 KiB')


def model_key(params, path):
    """Identify the immutable model before reopening FITS. / 先查模型缓存，再读取 FITS。

    Include the complete override and file replacement/change signature. Owners and
    document generations remain in Worker.header_owners, including shared models.
    完整坐标头、所选 HDU 和文件变化都属于缓存键；文档归属仍由 Worker 统一释放。
    """
    view = params.get('view') if isinstance(params.get('view'), dict) else {}
    text = params.get('wcs_override', view.get('wcs_override'))
    validate_override(text)
    selected = params.get('hdu')
    if selected is not None:
        if isinstance(selected, bool) or int(selected) != float(selected) or int(selected) < 0:
            raise ValueError('Image HDU must be a nonnegative integer')
        selected = int(selected)
    stat = path.stat()
    return (str(path), stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns,
            stat.st_ctime_ns, selected, text)


def overlay(header, text):
    if text is None:
        return header
    validate_override(text)
    from astropy.io.fits import Header
    from astropy.wcs import WCS
    replacement = Header.fromstring(text, sep='\n')
    for key in replacement:
        if key not in ('', 'COMMENT', 'HISTORY') and not _WCS_KEY.fullmatch(key):
            raise ValueError('WCS override cannot change data or metadata field: ' + key)
    wcs = WCS(replacement, naxis=2).celestial
    if not wcs.has_celestial:
        raise ValueError('WCS override must contain a usable two-dimensional celestial model')
    result = header.copy()
    for key in list(result):
        if _WCS_KEY.fullmatch(key) or _DSS_KEY.fullmatch(key) or key.startswith(('CPDIS', 'D2IM', 'DET2IM', 'DP1', 'DP2')):
            del result[key]
    for card in replacement.cards:
        if _WCS_KEY.fullmatch(card.keyword):
            result[card.keyword] = (card.value, card.comment)
    return result
