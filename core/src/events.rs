use spotify_id::SpotifyId;
use keymaster::Token;

#[derive(Debug, Clone)]
pub enum Event {
    SessionActive {
        became_active_at: i64,
    },
    SessionInactive {
        became_inactive_at: i64,
    },
    SinkActive,
    SinkInactive,
    GotToken {
        token: Token,
    },
    Load {
        track_id: SpotifyId,
    },
    Pause {
        track_id: SpotifyId,
        position_ms: u32,
    },
    Play {
        track_id: SpotifyId,
        position_ms: u32,
    },
    Next {
        track_id: SpotifyId,
    },
    Previous {
        track_id: SpotifyId,
    },
    Seek {
        position_ms: u32,
    },
    Volume {
        volume_to_mixer: u16,
    },
    Repeat {
        status: bool,
    },
    Shuffle {
        status: bool,
    },
    PlaybackStarted {
        track_id: SpotifyId,
    },
    PlaybackStopped {
        track_id: SpotifyId,
    },
    TrackChanged {
        old_track_id: SpotifyId,
        track_id: SpotifyId,
    },
}
