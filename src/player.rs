use futures::sync::oneshot;
use futures::{future, Future};
use std::borrow::Cow;
use std::mem;
use std::sync::mpsc::{RecvError, TryRecvError, RecvTimeoutError};
use std::thread;
use std::time::Duration;
use std;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Read;
// Metadata socket
use std::net::UdpSocket;
use std::io;
use keymaster;

use core::config::{Bitrate, PlayerConfig};
use core::session::Session;
use core::util::{self, SpotifyId, Subfile};

use audio_backend::Sink;
use audio::{AudioFile, AudioDecrypt};
use audio::{VorbisDecoder, VorbisPacket};
use metadata::{FileFormat, Track, Metadata,Artist,Album};
use mixer::AudioFilter;

pub struct Player {
    commands: Option<std::sync::mpsc::Sender<PlayerCommand>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

struct PlayerInternal {
    session: Session,
    config: PlayerConfig,
    commands: std::sync::mpsc::Receiver<PlayerCommand>,

    state: PlayerState,
    sink: Box<Sink>,
    sink_running: bool,
    audio_filter: Option<Box<AudioFilter + Send>>,
    token: keymaster::Token,
}

enum PlayerCommand {
    Load(SpotifyId, bool, u32, oneshot::Sender<()>),
    Play,
    Pause,
    Stop,
    Seek(u32),
}

impl Player {
    pub fn new<F>(config: PlayerConfig, session: Session,
                  audio_filter: Option<Box<AudioFilter + Send>>,
                  sink_builder: F) -> Player
        where F: FnOnce() -> Box<Sink> + Send + 'static
    {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

        let handle = thread::spawn(move || {
            debug!("new Player[{}]", session.session_id());

            let internal = PlayerInternal {
                session: session,
                config: config,
                commands: cmd_rx,

                state: PlayerState::Stopped,
                sink: sink_builder(),
                sink_running: false,
                audio_filter: audio_filter,
                token: keymaster::Token{access_token: "".to_string(), expires_in:0, token_type:"".to_string(), scope: vec!["".to_string(), "".to_string()]},
            };

            internal.run();
        });

        Player {
            commands: Some(cmd_tx),
            thread_handle: Some(handle),
        }
    }

    fn command(&self, cmd: PlayerCommand) {
        self.commands.as_ref().unwrap().send(cmd).unwrap();
    }

    pub fn load(&self, track: SpotifyId, start_playing: bool, position_ms: u32)
        -> oneshot::Receiver<()>
    {
        let (tx, rx) = oneshot::channel();
        self.command(PlayerCommand::Load(track, start_playing, position_ms, tx));

        rx
    }

    pub fn play(&self) {
        self.command(PlayerCommand::Play)
    }

    pub fn pause(&self) {
        self.command(PlayerCommand::Pause)
    }

    pub fn stop(&self) {
        self.command(PlayerCommand::Stop)
    }

    pub fn seek(&self, position_ms: u32) {
        self.command(PlayerCommand::Seek(position_ms));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        debug!("Shutting down player thread ...");
        self.commands = None;
        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(_) => (),
                Err(_) => error!("Player thread panicked!")
            }
        }
    }
}

type Decoder = VorbisDecoder<Subfile<AudioDecrypt<AudioFile>>>;
enum PlayerState {
    Stopped,
    Paused {
        decoder: Decoder,
        normalization_factor: f32,
        end_of_track: oneshot::Sender<()>,
    },
    Playing {
        decoder: Decoder,
        normalization_factor: f32,
        end_of_track: oneshot::Sender<()>,
    },

    Invalid,
}

impl PlayerState {
    fn is_playing(&self) -> bool {
        use self::PlayerState::*;
        match *self {
            Stopped | Paused { .. } => false,
            Playing { .. } => true,
            Invalid => panic!("invalid state"),
        }
    }

    fn decoder(&mut self) -> Option<&mut Decoder> {
        use self::PlayerState::*;
        match *self {
            Stopped => None,
            Paused { ref mut decoder, .. } |
            Playing { ref mut decoder, .. } => Some(decoder),
            Invalid => panic!("invalid state"),
        }
    }

    fn signal_end_of_track(self) {
        use self::PlayerState::*;
        match self {
            Paused { end_of_track, .. } |
            Playing { end_of_track, .. } => {
                let _ = end_of_track.send(());
                debug!("End of track!");
            }

            Stopped => warn!("signal_end_of_track from stopped state"),
            Invalid => panic!("invalid state"),
        }
    }

    fn paused_to_playing(&mut self) {
        use self::PlayerState::*;
        match ::std::mem::replace(self, Invalid) {
            Paused { decoder, normalization_factor, end_of_track } => {
                *self = Playing {
                    decoder: decoder,
                    normalization_factor: normalization_factor,
                    end_of_track: end_of_track,
                };
            }
            _ => panic!("invalid state"),
        }
    }

    fn playing_to_paused(&mut self) {
        use self::PlayerState::*;
        match ::std::mem::replace(self, Invalid) {
            Playing { decoder, normalization_factor, end_of_track } => {
                *self = Paused {
                    decoder: decoder,
                    normalization_factor: normalization_factor,
                    end_of_track: end_of_track,
                };
            }
            _ => panic!("invalid state"),
        }
    }
}

impl PlayerInternal {
    fn run(mut self) {
        loop {
            let cmd = if self.state.is_playing() {
                if self.sink_running
                {
                    match self.commands.try_recv() {
                        Ok(cmd) => Some(cmd),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                else
                {
                    match self.commands.recv_timeout(Duration::from_secs(5)) {
                        Ok(cmd) => Some(cmd),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            } else {
                match self.commands.recv() {
                    Ok(cmd) => Some(cmd),
                    Err(RecvError) => return,
                }
            };

            if let Some(cmd) = cmd {
                self.handle_command(cmd);
            }

            if self.state.is_playing() && ! self.sink_running {
                self.start_sink();
            }

            if self.sink_running {
                let mut current_normalization_factor: f32 = 1.0;
                let packet = if let PlayerState::Playing { ref mut decoder, normalization_factor, .. } = self.state {
                    current_normalization_factor = normalization_factor;
                    Some(decoder.next_packet().expect("Vorbis error"))
                } else {
                    None
                };

                if let Some(packet) = packet {
                    self.handle_packet(packet, current_normalization_factor);
                }
            }
        }
    }

    fn start_sink(&mut self) {
        match self.sink.start() {
            Ok(()) => { self.sink_running = true;
                        debug!("Sink Aquired!");
                        self.snd_meta(String::from("kSpDeviveActive"));
                      },
            Err(err) => error!("Could not start audio: {}", err),
        }
    }

    fn stop_sink_if_running(&mut self) {
        if self.sink_running {
            self.stop_sink();
        }
    }

    fn stop_sink(&mut self) {
        self.sink.stop().unwrap();
        debug!("Sink disconnected");
        self.snd_meta(String::from("kSpDeviveInactive"));
        self.sink_running = false;
    }

    fn handle_packet(&mut self, packet: Option<VorbisPacket>, normalization_factor: f32) {
        match packet {
            Some(mut packet) => {
                if let Some(ref editor) = self.audio_filter {
                    editor.modify_stream(&mut packet.data_mut())
                };
                if self.config.normalization {

                    if normalization_factor != 1.0 {
                        for x in packet.data_mut().iter_mut() {
                            *x = (*x as f32 * normalization_factor) as i16;
                        }
                    }

                }

                if let Err(err) = self.sink.write(&packet.data()) {
                    error!("Could not write audio: {}", err);
                    self.stop_sink();
                }
            }

            None => {
                // Signlling end of track, as there is no more data
                self.stop_sink();
                self.run_onstop();

                let old_state = mem::replace(&mut self.state, PlayerState::Stopped);
                old_state.signal_end_of_track();
            }
        }
    }

    fn handle_command(&mut self, cmd: PlayerCommand) {
        debug!("command={:?}", cmd);
        // Token::
        let client_id = env!("CLIENT_ID");
        let access    = "streaming,user-read-playback-state,user-modify-playback-state,user-read-currently-playing,user-read-private".to_string();
        match keymaster::get_token(&self.session, &client_id, &access).wait() {
            Ok(token) => self.token = token,
            Err(err)  => info!("Err: {:?}",err),
        }

        self.snd_meta(json!({"token":self.token}).to_string());
        match cmd {
            PlayerCommand::Load(track_id, play, position, end_of_track) => {
                // Device asked to load track -- Notify active session!
                self.snd_meta(String::from("kSpPlaybackNotifyBecameActive"));
                if self.state.is_playing() {
                    self.stop_sink_if_running();
                }

                match self.load_track(track_id, position as i64) {
                    Some((decoder, normalization_factor)) => {
                        if play {
                            if !self.state.is_playing() {
                                self.run_onstart();
                            }
                            self.start_sink();

                            // We should send the metadata now, as the sink has started
                            self.snd_meta(self.track_id_to_json(track_id, position));

                            self.state = PlayerState::Playing {
                                decoder: decoder,
                                normalization_factor: normalization_factor,
                                end_of_track: end_of_track,
                            };
                        } else {
                            if self.state.is_playing() {
                                self.run_onstop();
                            }

                            self.state = PlayerState::Paused {
                                decoder: decoder,
                                normalization_factor: normalization_factor,
                                end_of_track: end_of_track,
                            };
                        }
                    }

                    None => {
                        let _ = end_of_track.send(());
                        if self.state.is_playing() {
                            self.run_onstop();
                        }
                    }
                }
            }

            PlayerCommand::Seek(position) => {
                if let Some(decoder) = self.state.decoder() {
                    match decoder.seek(position as i64) {
                        Ok(_) => (),
                        Err(err) => error!("Vorbis error: {:?}", err),
                    }
                } else {
                    warn!("Player::seek called from invalid state");
                }
            }

            PlayerCommand::Play => {
                if let PlayerState::Paused { .. } = self.state {
                    self.state.paused_to_playing();

                    debug!("Play");
                    self.run_onstart();
                    self.start_sink();
                } else {
                    warn!("Player::play called from invalid state");
                }
            }

            PlayerCommand::Pause => {
                if let PlayerState::Playing { .. } = self.state {
                    self.state.playing_to_paused();

                    debug!("Pause");
                    self.stop_sink_if_running();
                    self.run_onstop();
                } else {
                    warn!("Player::pause called from invalid state");
                }
            }

            PlayerCommand::Stop => {
                match self.state {
                    PlayerState::Playing { .. } => {
                        self.stop_sink_if_running();
                        self.run_onstop();
                        self.state = PlayerState::Stopped;
                    }
                    PlayerState::Paused { .. } => {
                        self.state = PlayerState::Stopped;
                    },
                    PlayerState::Stopped => {
                        warn!("Player::stop called from invalid state");
                    }
                    PlayerState::Invalid => panic!("invalid state"),
                }
            }
        }
    }

    // 10 mins of google foo, with no Rust background gives::
    fn snd_udp(&self, msg: String) -> Result<(), io::Error> {
        let socket = try!(UdpSocket::bind("127.0.0.1:5031"));

        try!(socket.send_to(msg.as_bytes(),"127.0.0.1:5030"));

        Ok(())
    }

    fn snd_meta(&self, meta: String) {
        match self.snd_udp(meta){
            Ok(_) => (),
            Err(err) => println!("Error: {:?}", err),
        }
    }

    fn track_id_to_json(&self, track_id: SpotifyId, position: u32) -> String {
        // This function should idealy return a serde_json::Value type, not string
        // to make adding more metadata convinient
        let track  = Track::get(&self.session, track_id).wait().unwrap();
        let artist = Artist::get(&self.session,track.artists[0]).wait().unwrap();
        let album = Album::get(&self.session,track.album).wait().unwrap();

        let meta_json = json!(
            {"metadata" : {
                            "track_id": track_id.to_base16(),
                            "track_name": track.name,
                            "artist_id":artist.id.to_base16(),
                            "artist_name": artist.name,
                            "album_id": album.id.to_base16(),
                            "album_name": album.name,
                            "duration": track.duration,
                            // type FileId.to_base16() string
                            "albumartId": album.covers[0].to_base16(),
                            "albumartId_SMALL": album.covers[1].to_base16(),
                            "albumartId_LARGE": album.covers[2].to_base16(),
                            "pos": position,
                        }});
        return meta_json.to_string();
    }

    fn run_onstart(&self) {
        debug!("onStart");
        if let Some(ref program) = self.config.onstart {
            util::run_program(program)
        }
    }

    fn run_onstop(&self) {
        debug!("onStop");
        if let Some(ref program) = self.config.onstop {
            util::run_program(program)
        }
    }

    fn find_available_alternative<'a>(&self, track: &'a Track) -> Option<Cow<'a, Track>> {
        if track.available {
            Some(Cow::Borrowed(track))
        } else {
            let alternatives = track.alternatives
                .iter()
                .map(|alt_id| {
                    Track::get(&self.session, *alt_id)
                });
            let alternatives = future::join_all(alternatives).wait().unwrap();

            alternatives.into_iter().find(|alt| alt.available).map(Cow::Owned)
        }
    }

    fn load_track(&self, track_id: SpotifyId, position: i64) -> Option<(Decoder, f32)> {
        let track  = Track::get(&self.session, track_id).wait().unwrap();
        let artist = Artist::get(&self.session,track.artists[0]).wait().unwrap();
        let album = Album::get(&self.session,track.album).wait().unwrap();

        debug!("Loading track \"{}\" by {} from {}", track.name,artist.name,album.name);

        let track = match self.find_available_alternative(&track) {
            Some(track) => track,
            None => {
                warn!("Track \"{}\" is not available", track.name);
                return None;
            }
        };

        let format = match self.config.bitrate {
            Bitrate::Bitrate96 => FileFormat::OGG_VORBIS_96,
            Bitrate::Bitrate160 => FileFormat::OGG_VORBIS_160,
            Bitrate::Bitrate320 => FileFormat::OGG_VORBIS_320,
        };

        let file_id = match track.files.get(&format) {
            Some(&file_id) => file_id,
            None => {
                warn!("Track \"{}\" is not available in format {:?}", track.name, format);
                return None;
            }
        };

        let key = self.session.audio_key().request(track.id, file_id).wait().unwrap();

        let encrypted_file = AudioFile::open(&self.session, file_id).wait().unwrap();
        let mut decrypted_file = AudioDecrypt::new(key, encrypted_file);

        let mut normalization_factor: f32 = 1.0;

        if self.config.normalization {
            //buffer for float bytes
            let mut track_gain_float_bytes = [0; 4];

            decrypted_file.seek(SeekFrom::Start(144)).unwrap(); // 4 bytes as LE float
            decrypted_file.read(&mut track_gain_float_bytes).unwrap();
            let track_gain_db: f32;
            unsafe {
                track_gain_db = mem::transmute::<[u8; 4], f32>(track_gain_float_bytes);
                debug!("Track gain: {}db", track_gain_db);
            }

            decrypted_file.seek(SeekFrom::Start(148)).unwrap(); // 4 bytes as LE float
            decrypted_file.read(&mut track_gain_float_bytes).unwrap();
            let track_peak: f32;
            unsafe {
                // track peak, 1.0 represents dbfs
                track_peak = mem::transmute::<[u8; 4], f32>(track_gain_float_bytes);
                debug!("Track peak: {}", track_peak);
            }

            // see http://wiki.hydrogenaud.io/index.php?title=ReplayGain_specification#Loudness_normalization
            normalization_factor = f32::powf(10.0, (track_gain_db + self.config.normalization_pregain) / 20.0);

            if normalization_factor * track_peak > 1.0 {
                warn!("Track would clip, reducing normalization factor. \
                    Please add negative pre-gain to avoid.");
                normalization_factor = 1.0/track_peak;
            }

            debug!("Applying normalization factor: {}", normalization_factor);

            // TODO there are also values for album gain/peak, which should be used if an album is playing
            // but I don't know how to determine if album is playing
            decrypted_file.seek(SeekFrom::Start(152)).unwrap(); // 4 bytes as LE float
            decrypted_file.read(&mut track_gain_float_bytes).unwrap();
            unsafe {
                debug!("Album gain: {}db", mem::transmute::<[u8; 4], f32>(track_gain_float_bytes));
            }
            decrypted_file.seek(SeekFrom::Start(156)).unwrap(); // 4 bytes as LE float
            decrypted_file.read(&mut track_gain_float_bytes).unwrap();
            unsafe {
                // album peak, 1.0 represents dbfs
                debug!("Album peak: {}", mem::transmute::<[u8; 4], f32>(track_gain_float_bytes));
            }
        }
        let audio_file = Subfile::new(decrypted_file, 0xa7);
        let mut decoder = VorbisDecoder::new(audio_file).unwrap();

        match decoder.seek(position) {
            Ok(_) => (),
            Err(err) => error!("Vorbis error: {:?}", err),
        }

        info!("Loaded Track \"{}\" by {} from {}", track.name,artist.name,album.name);

        Some((decoder, normalization_factor))
    }
}

impl Drop for PlayerInternal {
    fn drop(&mut self) {
        debug!("drop Player[{}]", self.session.session_id());
    }
}

impl ::std::fmt::Debug for PlayerCommand {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            PlayerCommand::Load(track, play, position, _) => {
                f.debug_tuple("Load")
                 .field(&track)
                 .field(&play)
                 .field(&position)
                 .finish()
            }
            PlayerCommand::Play => {
                f.debug_tuple("Play").finish()
            }
            PlayerCommand::Pause => {
                f.debug_tuple("Pause").finish()
            }
            PlayerCommand::Stop => {
                f.debug_tuple("Stop").finish()
            }
            PlayerCommand::Seek(position) => {
                f.debug_tuple("Seek")
                 .field(&position)
                 .finish()
            }
        }
    }
}
