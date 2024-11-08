/*
* Convert data received from the ULD C API to more.. robust formats:
*   - 1D vectors -> 2D matrices
*   - integers -> enums or tuple structs
*   - some squeezing of type safety, e.g. negative 'distance_mm's not accepted
*
* Note: It is by design that these conversions happen already at the ULD level.
*
* Note: Many of the individual data are steered by features. These go all the way to the C level:
*       disabling a feature means less driver code, less data to transfer.
*
* References:
*   - vendor's UM2884 > Chapter 5 ("Ranging results"); Rev 5, Feb'24; PDF 18pp.
*       -> https://www.st.com/resource/en/user_manual/um2884-a-guide-to-using-the-vl53l5cx-multizone-timeofflight-ranging-sensor-with-a-wide-field-of-view-ultra-lite-driver-uld-stmicroelectronics.pdf
*/
use core::convert::identity;
//Ruse core::mem;

#[cfg(feature = "defmt")]
use defmt::{assert};

use crate::uld_raw::{
    VL53L5CX_ResultsData,
};
use crate::units::TempC;

// Note: We could also take in 'TARGETS_PER_ZONE' from the ULD C API wrapper.
const TARGETS: usize =
         if cfg!(feature = "targets_per_zone_4") { 4 }
    else if cfg!(feature = "targets_per_zone_3") { 3 }
    else if cfg!(feature = "targets_per_zone_2") { 2 }
    else { 1 };

/*
* Results data, in matrix format.
*
* Note: Scalar metadata ('silicon_temp_degc') that ULD C API treats as a result is being delivered
*       separately. This is mainly a matter of taste: many of the matrix "results" are actually
*       also metadata. Only '.distance_mm' and (likely) '.reflectance_percent' can be seen as
*       actual results. It doesn't really matter.
*/
pub struct ResultsData<const DIM: usize> {      // DIM: 4,8
    // Metadata: DIMxDIM matrix, regardless of 'TARGETS'
    //
    #[cfg(feature = "ambient_per_spad")]
    pub ambient_per_spad: [[u32; DIM]; DIM],
    #[cfg(feature = "nb_spads_enabled")]
    pub spads_enabled: [[u32; DIM]; DIM],
    #[cfg(feature = "nb_targets_detected")]
    pub targets_detected: [[u8; DIM]; DIM],     // 1..{X in 'targets_per_zone_X' feature}

    // Actual results: DIMxDIMxTARGETS
    #[cfg(feature = "target_status")]
    pub target_status: [[[TargetStatus; DIM]; DIM]; TARGETS],

    #[cfg(feature = "distance_mm")]
    pub distance_mm: [[[u16; DIM]; DIM]; TARGETS],
    #[cfg(feature = "range_sigma_mm")]
    pub range_sigma_mm: [[[u16; DIM]; DIM]; TARGETS],

    #[cfg(feature = "reflectance_percent")]
    pub reflectance: [[[u8; DIM]; DIM]; TARGETS],
    #[cfg(feature = "signal_per_spad")]
    pub signal_per_spad: [[[u32; DIM]; DIM]; TARGETS],
}

impl<const DIM: usize> ResultsData<DIM> {
    /*
    * Provide an empty buffer-like struct; owned usually by the application and fed via 'feed()'.
    */
    fn empty() -> Self {

        Self {
            #[cfg(feature = "ambient_per_spad")]
            ambient_per_spad: [[0;DIM];DIM],
            #[cfg(feature = "nb_spads_enabled")]
            spads_enabled: [[0;DIM];DIM],
            #[cfg(feature = "nb_targets_detected")]
            targets_detected: [[0;DIM];DIM],

            #[cfg(feature = "target_status")]
            target_status: [[[TargetStatus::Other(0);DIM];DIM];TARGETS],

            #[cfg(feature = "distance_mm")]
            distance_mm: [[[0;DIM];DIM];TARGETS],
            #[cfg(feature = "range_sigma_mm")]
            range_sigma_mm: [[[0;DIM];DIM];TARGETS],

            #[cfg(feature = "signal_per_spad")]
            signal_per_spad: [[[0;DIM];DIM];TARGETS],
            #[cfg(feature = "reflectance_percent")]
            reflectance: [[[0;DIM];DIM];TARGETS],
        }
    }

    pub(crate) fn from(raw_results: &VL53L5CX_ResultsData) -> (Self,TempC) {
        // tbd. Implement using 'MaybeUninit'; started but left..wasn't as easy as hoped.
        let mut x = Self::empty();
        let tempC = x.feed(raw_results);
        (x, tempC)
    }

    fn feed(&mut self, raw_results: &VL53L5CX_ResultsData) -> TempC {

        // helpers
        //
        // The ULD C API matrix layout is,
        //  - looking _out_ through the sensor so that the SATEL mini-board's PCB text is horizontal
        //    and right-way-up
        //      ^-- i.e. what the sensor "sees" (not how we look at the sensor)
        //  - for a fictional 2x2x2 matrix = only the corner zones
        //
        // Real world:
        //      [A B]   // A₁..D₁ = first targets; A₂..D₂ = 2nd targets; i.e. same target zone
        //      [C D]
        //
        // ULD C API vector:
        //      [A₁ A₂ B₁ B₂ C₁ C₂ D₁ D₂]   // every "zone" is first covered; then next zone
        //
        #[allow(dead_code)]
        fn into_matrix_map_o<IN: Copy, OUT, const DIM: usize>(raw: &[IN], offset: usize, out: &mut [[OUT; DIM]; DIM], f: impl Fn(IN) -> OUT) {
            let raw = &raw[..DIM * DIM * TARGETS];      // take only the beginning of the C buffer

            for r in 0..DIM {
                for c in 0..DIM {
                    out[r][c] = f(raw[(r * DIM + c) * TARGETS + offset]);
                }
            }
        }
        #[inline]
        #[allow(dead_code)]
        fn into_matrix_o<X: Copy, const DIM: usize>(raw: &[X], offset: usize, out: &mut [[X; DIM]; DIM]) {     // no mapping
            into_matrix_map_o(raw, offset, out, identity)
        }
        // Zone metadata: 'TARGETS' (and 'offset', by extension) are not involved.
        fn into_matrix<X: Copy, const DIM: usize>(raw: &[X], out: &mut [[X; DIM]; DIM]) {
            let raw = &raw[..DIM * DIM];      // take only the beginning of the C buffer

            for r in 0..DIM {
                for c in 0..DIM {
                    out[r][c] = raw[r*DIM+c];
                }
            }
        }

        // Metadata: DIMxDIM (just once)
        //
        #[cfg(feature = "ambient_per_spad")]
        into_matrix(&raw_results.ambient_per_spad, &mut self.ambient_per_spad);
        #[cfg(feature = "nb_spads_enabled")]
        into_matrix(&raw_results.nb_spads_enabled, &mut self.spads_enabled);
        #[cfg(feature = "nb_targets_detected")]
        into_matrix(&raw_results.nb_target_detected, &mut self.targets_detected);

        // Results: DIMxDIMxTARGETS
        //
        for i in 0..TARGETS {
            #[cfg(feature = "target_status")]
            into_matrix_map_o(&raw_results.target_status, i, &mut self.target_status[i], TargetStatus::from_uld);

            // We tolerate '.distance_mm' == 0 for non-existing data (where '.target_status' is 0); no need to check.
            //
            #[cfg(feature = "distance_mm")]
            into_matrix_map_o(&raw_results.distance_mm, i, &mut self.distance_mm[i],
            |v: i16| -> u16 {
                assert!(v >= 0, "Unexpected 'distance_mm' value: {} < 0", v); v as u16
            });
            #[cfg(feature = "range_sigma_mm")]
            into_matrix_o(&raw_results.range_sigma_mm, i, &mut self.range_sigma_mm[i]);

            #[cfg(feature = "reflectance_percent")]
            into_matrix_o(&raw_results.reflectance, i, &mut self.reflectance[i]);
            #[cfg(feature = "signal_per_spad")]
            into_matrix_o(&raw_results.signal_per_spad, i, &mut self.signal_per_spad[i]);
        }

        TempC(raw_results.silicon_temp_degc)
    }
}
/*** WIP; Would be nice to have it just return a 'Self'
    - ended up in problems with '&mut [[X;DIM];DIM]' not being a "thing"..

pub(crate) fn from(raw_results: &VL53L5CX_ResultsData) -> (Self,TempC) {
    use mem::MaybeUninit;
    use core::ptr::addr_of_mut;

    // tbd. could take a time stamp already here, but that means bringing up some dependency
    //      the ULD side otherwise wouldn't need ('fugit'). #consider
    //
    trace!("Converting result on ULD side");

    // helpers
    //
    // The ULD C API matrix layout is,
    //  - looking _out_ through the sensor so that the SATEL mini-board's PCB text is horizontal
    //    and right-way-up
    //      ^-- i.e. what the sensor "sees" (not how we look at the sensor)
    //  - for a fictional 2x2x2 matrix = only the corner zones
    //
    // Real world:
    //      [A B]   // A₁..D₁ = first targets; A₂..D₂ = 2nd targets; i.e. same target zone
    //      [C D]
    //
    // ULD C API vector:
    //      [A₁ A₂ B₁ B₂ C₁ C₂ D₁ D₂]   // every "zone" is first covered; then next zone

    // RUST note: Cannot use '&[IN;DIM*DIM]' (or '&[IN;DIM_SQ]'), which would technically be
    //      correct.
    //      <<
    //          error: generic parameters may not be used in const operations
    //      <<
    //
    #[allow(dead_code)]
    fn into_matrix_map_o<IN: Copy, OUT, const DIM: usize>(raw: &[IN], offset: usize, out: &mut [[OUT; DIM]; DIM], f: impl Fn(IN) -> OUT) {
        let raw = &raw[..DIM * DIM * TARGETS];      // take only the beginning of the C buffer

        for r in 0..DIM {
            for c in 0..DIM {
                out[r][c] = f(raw[(r * DIM + c) * TARGETS + offset]);
                //(unsafe { out.add(r*DIM+c) }) = f(raw[(r * DIM + c) * TARGETS + offset]);
            }
        }
    }
    #[inline]
    #[allow(dead_code)]
    fn into_matrix_o<X: Copy, const DIM: usize>(raw: &[X], offset: usize, out: &mut [[X; DIM]; DIM]) {     // no mapping
        into_matrix_map_o(raw, offset, out, identity)
    }
    // Zone metadata: 'TARGETS' (and 'offset', by extension) are not involved.
    fn into_matrix<X: Copy, const DIM: usize>(raw: &[X], out: &mut [[X; DIM]; DIM]) {
        let raw = &raw[..DIM * DIM];      // take only the beginning of the C buffer

        // tbd.
        // Since we cannot use 2D indexes with the pointer (was able to, with a reference),
        // and since the layout _might_ be the same, just a memcopy would do?
        for r in 0..DIM {
            for c in 0..DIM {
                out[r][c] = raw[r*DIM+c];
                //(unsafe { out.add(r*DIM+c) }) = raw[r*DIM+c];
            }
        }
    }

    // Ref -> https://doc.rust-lang.org/beta/std/mem/union.MaybeUninit.html#initializing-a-struct-field-by-field
    //
    let rd: ResultsData<DIM> = {
        let mut un = MaybeUninit::<Self>::uninit();
        let up = un.as_mut_ptr();

        let rr = raw_results;    // alias

        // Metadata: DIMxDIM (just once)
        //
        #[cfg(feature = "ambient_per_spad")]
        into_matrix(&rr.ambient_per_spad, unsafe { addr_of_mut!((*up).ambient_per_spad) });
        #[cfg(feature = "nb_spads_enabled")]
        into_matrix(&rr.spads_enabled, unsafe { addr_of_mut!((*up).nb_spads_enabled) });
        #[cfg(feature = "nb_targets_detected")]
        into_matrix(&rr.nb_target_detected, unsafe { addr_of_mut!((*up).targets_detected) });

        // Results: DIMxDIMxTARGETS
        //
        for i in 0..TARGETS {
            #[cfg(feature = "target_status")]
            into_matrix_map_o(&rr.target_status, i, unsafe { addr_of_mut!((*up).target_status[i]) }, TargetStatus::from_uld);

            // We tolerate '.distance_mm' == 0 for non-existing data (where '.target_status' is 0); no need to check.
            //
            #[cfg(feature = "distance_mm")]
            into_matrix_map_o(&rr.distance_mm, i, unsafe { addr_of_mut!((*up).distance_mm[i]) },
                              |v: i16| -> u16 {
                                  assert!(v >= 0, "Unexpected 'distance_mm' value: {} < 0", v);
                                  v as u16
                              });
            #[cfg(feature = "range_sigma_mm")]
            into_matrix_o(&rr.range_sigma_mm, i, unsafe { addr_of_mut!((*up).range_sigma_mm[i]) });

            #[cfg(feature = "reflectance_percent")]
            into_matrix_o(&rr.reflectance, i, unsafe { addr_of_mut!((*up).reflectance[i]) });
            #[cfg(feature = "signal_per_spad")]
            into_matrix_o(&rr.signal_per_spad, i, unsafe { addr_of_mut!((*up).signal_per_spad[i]) });
        }
        unsafe { un.assume_init() }
    };
    let tempC = TempC(raw_results.silicon_temp_degc);

    (rd, tempC)
}
***/

//---
// Target status
//
// Observed values:
//      5, 6, 9, 10, 255
//
// Note: Vendor docs (section 5.5.; Table 4) give detailed explanations for values 0..13 and 255.
//      They are regarded as not relevant enough to surface on the level of 'enum's. Applications
//      can access them though, as the inner values.
//
#[cfg(feature = "target_status")]
#[derive(Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TargetStatus {
    Valid(u8),          // 100% valid: 5
    HalfValid(u8),      // 50% valid: 6,9
    Invalid,            // 255
    Other(u8),          // other values: 0..13 excluding above; RARE

    // 14..254 (inclusive); should not occur; panics
}

#[cfg(feature = "target_status")]
impl TargetStatus {
    fn from_uld(v: u8) -> Self {
        match v {
            5 => { Self::Valid(v) },
            6 | 9 => { Self::HalfValid(v) },
            255 => { Self::Invalid },
            0..=13 => { Self::Other(v) },
            v => panic!("Unexpected value {} for target status", v),
        }
    }
}
