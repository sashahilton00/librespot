use super::{Open, Sink};
use alsa::{Direction, ValueOr, Error};
use alsa::pcm::{Access, Format, HwParams, PCM};
use std::io;

pub struct AlsaSink(Option<PCM>, String);

fn open_device(dev_name: &str) -> Result<(PCM), Box<Error>>  {
    let pcm = PCM::new(dev_name, Direction::Playback, false)?;
    // plietar sets latency = 500000u32,
    // http://www.linuxjournal.com/article/6735?page=0,1#N0x19ab2890.0x19ba78d8
    // latency = periodsize * periods / (rate * bytes_per_frame)
    // For 16 Bit stereo data, one frame has a length of four bytes.
    // ~ 20ms = periodsize * 2 / (44100 * 4)
    let periodsize: i32 = 16396 ;
    {
        // Set hardware parameters: 44100 Hz / Stereo / 16 bit
        let hwp = HwParams::any(&pcm)?;
        hwp.set_channels(2)?;
        hwp.set_rate(44100, ValueOr::Nearest)?;
        hwp.set_format(Format::s16())?;
        hwp.set_buffer_size(periodsize * 2)?;
        hwp.set_period_size(periodsize, ValueOr::Nearest)?;
        hwp.set_access(Access::RWInterleaved)?;
        pcm.hw_params(&hwp)?;
    }

    Ok((pcm))
}

impl Open for AlsaSink {
    fn open(device: Option<String>) -> AlsaSink {
        info!("Using alsa sink");

        let name = device.unwrap_or("default".to_string());

        AlsaSink(None, name)
    }
}

impl Sink for AlsaSink {
    fn start(&mut self) -> io::Result<()> {
        if self.0.is_none() {
            let pcm = open_device(&self.1);
            match pcm {
                Ok(p) => self.0 = Some(p),
                Err(e) => {
                        error!("Alsa error PCM open {}", e);
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Alsa error: PCM open failed",
                        ));
                    }
            }
            // //TODO Add proper error checking!
            // let pcm = PCM::new(&*self.1, Direction::Playback, false).unwrap();
            // {
            //     // Set hardware parameters: 44100 Hz / Stereo / 16 bit
            //     let hwp = HwParams::any(&pcm).unwrap();
            //     hwp.set_channels(2).unwrap();
            //     hwp.set_rate(44100, ValueOr::Nearest).unwrap();
            //     hwp.set_format(Format::s16()).unwrap();
            //     hwp.set_access(Access::RWInterleaved).unwrap();
            //     pcm.hw_params(&hwp).unwrap();
            // }
            // self.0 = Some(pcm);
            // match pcm {
            //     Ok(p) =>
            //     Err(e) => {
            //         error!("Alsa error PCM open {}", e);
            //         return Err(io::Error::new(
            //             io::ErrorKind::Other,
            //             "Alsa error: PCM open failed",
            //         ));
            //     }
            // }
        }
        Ok(())
    }

    fn stop(&mut self) -> io::Result<()> {
        self.0 = None;
        Ok(())
    }

    fn write(&mut self, data: &[i16]) -> io::Result<()> {
        let pcm = self.0.as_mut().unwrap();
        let io = pcm.io_i16().unwrap();

        match io.writei(&data) {
            Ok(_) => (),
            Err(err) => pcm.try_recover(err, false).unwrap(),
        }
        Ok(())
    }
}
