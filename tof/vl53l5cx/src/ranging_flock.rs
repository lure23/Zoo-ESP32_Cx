/*
* Scanning multiple VL53L5CX sensors for the next result.
*/
#![cfg(feature = "flock")]

#[cfg(feature = "defmt")]
use defmt::{debug,trace};

use esp_hal::{
    gpio::Input,
    time::Instant
};
use esp_hal::time::now;
use vl53l5cx_uld::{
    units::TempC,
    RangingConfig,
    Result,
    ResultsData,
    State_Ranging,
};

use arrayvec::ArrayVec;

use crate::{
    VL,
    z_array_try_map::turn_to_something
};

/*
* State for scanning multiple VL53L5CX boards.
*
* Note: A generator would be ideal for this (could keep the state within it).
*/
pub struct RangingFlock<const N: usize, const DIM: usize> {
    ulds: [State_Ranging<DIM>;N],
    pinINT: Input<'static>,
    pending: ArrayVec<(usize,ResultsData<DIM>,TempC,Instant),N>    // tbd. pick suitable capacity once we know the behaviour
}

impl<const N: usize, const DIM: usize> RangingFlock<N,DIM> {

    pub(crate) fn start(vls: [VL;N], cfg: &RangingConfig<DIM>, pinINT: Input<'static>) -> Result<Self> {

        // Turn the ULD level handles into "ranging" state, and start tracking the 'pinINT'.

        let ulds: [State_Ranging<DIM>;N] = turn_to_something(vls, |x| x.into_uld().start_ranging(cfg))?;

        Ok(Self{
            ulds,
            pinINT,
            pending: ArrayVec::new()
        })
    }

    /*
    * Get the next available results.
    *
    * By design, we provide just one result at a time. This is akin to streaming/generation, and
    *       makes it easier for the recipient, compared to getting 1..N results, at once.
    */
    pub async fn get_data(&mut self) -> Result<(usize,ResultsData<DIM>,TempC,Instant)> {

        // Time stamp the results as fast after knowing they exist, as possible.

        // 1. Anything in the 'pending'? Give them first.
        // 2. Check for new results
        // 3. If nothing, wait for the next INT lowering edge
        // 4. Also read new results on rising edge of INT: since we share the INT signal across
        //    boards, new entries may have turned up, in the mean time.
        //
        // Note: All of this logic is experimental. Let's trace what happens in reality, and
        //      adjust!
        //      - some measurements may get lost, but the number should be minimal
        //      - if two measurements from the same board, the older one shall never replace the newer one
        //        (they can be both delivered)
        //      - time stamps should be as close to actual measurement as possible!

        // Trace if we see new data
        #[cfg(not(all()))]
        {
            for (i,uld) in self.ulds.iter_mut().enumerate() {
                if uld.is_ready()? {
                    debug!("Data available on entry: {}", i);
                }
            }
        }

        assert!(self.ulds.len() > 1);   // TEMP
        loop {
            // Add new results to the 'self.pending'.
            for (i,uld) in self.ulds.iter_mut().enumerate().rev() {
                if uld.is_ready()? {
                    let time_stamp = now();
                    let (rd,tempC) = uld.get_data()?;

                    debug!("New data from #{}, pending becomes {}", i, self.pending.len()+1);
                    self.pending.push((i,rd,tempC,time_stamp));
                } else {
                    debug!("No new data from #{}", i);
                }
            }

            // Return already pending results, one at a time.
            if let Some(tuple) = self.pending.pop() {
                return Ok(tuple);
            }

            // No data; sleep until either edge
            //
            // Falling edge: VM has gotten new data
            // Rising edge: since we use same INT for all sensors, it might make sense to check
            //      this edge as well. If we are fast enough to fall in sleep before the INT-low
            //      ends (100us from the last new result), it's possible there's yet more data we
            //      didn't hear of. Checking both edges ensures we get even those, with sub-ms delay.
            //
            assert!(self.pending.is_empty());
            {
                trace!("Going to sleep (INT {}).", if self.pinINT.is_low() {"still low"} else {"high"});

                let t0 = now();
                self.pinINT.wait_for_any_edge().await;

                debug!("Woke up to INT edge (now {}; slept {}ms)", if self.pinINT.is_low() {"low"} else {"high"}, (now() - t0).to_millis());
            }
        }
    }

    pub fn stop(self) -> Result<([VL;N], Input<'static>)> {
        let vls = turn_to_something(self.ulds, |x| {
            let uld = x.stop()?;
            Ok( VL::recreate(uld) )
        })?;

        Ok( (vls, self.pinINT) )
    }
}
