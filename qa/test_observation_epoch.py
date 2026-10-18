"""Legacy FITS dates and the shared Gaia calibration/catalogue epoch contract.

旧 FITS 日期、世纪、非法元数据与自行运动的数值回归；不访问在线服务。
"""
from pathlib import Path
import sys,unittest,warnings

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'web/python'))
import numpy as np
import astropy.units as u
from astropy.coordinates import SkyCoord
from astropy.table import Table
from astropy.time import Time
from fitsviewer_gaia import observation_time,reference_sky


class ObservationEpoch(unittest.TestCase):
    def test_old_fits_year_is_always_twentieth_century(self):
        for old,iso in [('29/11/51','1951-11-29'),('01/01/00','1900-01-01'),
                        ('31/12/68','1968-12-31'),('01/01/69','1969-01-01'),
                        ('31/12/99','1999-12-31'),('29/02/52','1952-02-29')]:
            with self.subTest(old=old),warnings.catch_warnings():
                warnings.simplefilter('ignore')
                self.assertEqual(observation_time(old).isot[:10],iso)

    def test_modern_and_explicit_julian_epochs(self):
        for value in ['2026-10-19','2026-10-19T01:23:45.123','J2000',2016.,'2016.0']:
            with self.subTest(value=value):
                expected=Time(float(value),format='jyear') if isinstance(value,(int,float)) or value=='2016.0' else Time(value)
                self.assertAlmostEqual(float(observation_time(value).jd),float(expected.jd),places=9)

    def test_invalid_dates_are_short_errors_not_astropy_format_dump(self):
        for value in ['31/02/51','29/02/00','29/13/51','invalid',True,None,[],float('nan'),float('inf')]:
            with self.subTest(value=value),warnings.catch_warnings():
                warnings.simplefilter('ignore')
                with self.assertRaisesRegex(ValueError,'Observation date is invalid') as error:
                    observation_time(value)
                self.assertNotIn('\n',str(error.exception))

    def test_shared_gaia_epoch_propagation_matches_astropy(self):
        table=Table({'ra':[132.833,132.834],'dec':[11.818,11.819],
                     'pmra':[120.,np.nan],'pmdec':[-85.,np.nan],'ref_epoch':[2016.,2016.]})
        header={'DATE-OBS':'29/11/51'}
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            actual,note=reference_sky(table,header,{})
            oracle=SkyCoord(132.833*u.deg,11.818*u.deg,pm_ra_cosdec=120.*u.mas/u.yr,
                            pm_dec=-85.*u.mas/u.yr,obstime=Time(2016.,format='jyear'))
            oracle=oracle.apply_space_motion(new_obstime=Time('1951-11-29'))
        self.assertLess(float(actual[0].separation(oracle).arcsec),1e-7)
        self.assertAlmostEqual(actual[1].ra.deg,table['ra'][1],places=12)
        self.assertIn('1951-11-29',note)
        self.assertIn('1 rows lack',note)
        self.assertEqual(header,{'DATE-OBS':'29/11/51'})
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            _,override=reference_sky(table,header,{'observation_epoch':'2026-10-19'})
        self.assertIn('2026-10-19',override)

    def test_missing_epoch_or_motions_do_not_invent_propagation(self):
        simple=Table({'ra':[132.833],'dec':[11.818]})
        for header in [{},{'DATE-OBS':'invalid'}]:
            sky,note=reference_sky(simple,header,{})
            self.assertAlmostEqual(sky[0].ra.deg,132.833,places=12)
            self.assertIn('reference epoch',note)
        full=Table({'ra':[132.833],'dec':[11.818],'pmra':[120.],'pmdec':[-85.],'ref_epoch':[2016.]})
        _,note=reference_sky(full,{'DATE-OBS':' '},{})
        self.assertIn('unavailable',note)


if __name__=='__main__':unittest.main()
