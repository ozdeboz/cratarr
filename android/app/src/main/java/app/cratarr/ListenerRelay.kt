package app.cratarr

import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.DeviceInfo
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Metadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.text.Cue
import androidx.media3.common.text.CueGroup
import androidx.media3.common.util.UnstableApi

/**
 * A [Player.Listener] that hands every event on to [to]; a subclass changes the ones it means to.
 *
 * Written out because Kotlin's `by` does not hand on Java default methods, and every method of [Player.Listener] is one:
 * `Player.Listener by l` compiles, and all it did not override went nowhere. The media session behind such a wrapper never
 * heard of a queue or a track, so it posted no notification and its service never came to the foreground, and Android was
 * free to stop the music. [ListenerRelayTest] fails when Media3 adds a method this does not hand on.
 */
@OptIn(UnstableApi::class)
@Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
open class ListenerRelay(private val to: Player.Listener) : Player.Listener {
    override fun onEvents(player: Player, events: Player.Events) = to.onEvents(player, events)

    override fun onTimelineChanged(timeline: Timeline, reason: Int) = to.onTimelineChanged(timeline, reason)

    override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) = to.onMediaItemTransition(mediaItem, reason)

    override fun onTracksChanged(tracks: Tracks) = to.onTracksChanged(tracks)

    override fun onMediaMetadataChanged(mediaMetadata: MediaMetadata) = to.onMediaMetadataChanged(mediaMetadata)

    override fun onPlaylistMetadataChanged(mediaMetadata: MediaMetadata) = to.onPlaylistMetadataChanged(mediaMetadata)

    override fun onIsLoadingChanged(isLoading: Boolean) = to.onIsLoadingChanged(isLoading)

    override fun onLoadingChanged(isLoading: Boolean) = to.onLoadingChanged(isLoading)

    override fun onAvailableCommandsChanged(availableCommands: Player.Commands) = to.onAvailableCommandsChanged(availableCommands)

    override fun onTrackSelectionParametersChanged(parameters: TrackSelectionParameters) = to.onTrackSelectionParametersChanged(parameters)

    override fun onPlayerStateChanged(playWhenReady: Boolean, playbackState: Int) = to.onPlayerStateChanged(playWhenReady, playbackState)

    override fun onPlaybackStateChanged(playbackState: Int) = to.onPlaybackStateChanged(playbackState)

    override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) = to.onPlayWhenReadyChanged(playWhenReady, reason)

    override fun onPlaybackSuppressionReasonChanged(playbackSuppressionReason: Int) = to.onPlaybackSuppressionReasonChanged(playbackSuppressionReason)

    override fun onIsPlayingChanged(isPlaying: Boolean) = to.onIsPlayingChanged(isPlaying)

    override fun onRepeatModeChanged(repeatMode: Int) = to.onRepeatModeChanged(repeatMode)

    override fun onShuffleModeEnabledChanged(shuffleModeEnabled: Boolean) = to.onShuffleModeEnabledChanged(shuffleModeEnabled)

    override fun onPlayerError(error: PlaybackException) = to.onPlayerError(error)

    override fun onPlayerErrorChanged(error: PlaybackException?) = to.onPlayerErrorChanged(error)

    override fun onPositionDiscontinuity(reason: Int) = to.onPositionDiscontinuity(reason)

    override fun onPositionDiscontinuity(oldPosition: Player.PositionInfo, newPosition: Player.PositionInfo, reason: Int) =
        to.onPositionDiscontinuity(oldPosition, newPosition, reason)

    override fun onPlaybackParametersChanged(playbackParameters: PlaybackParameters) = to.onPlaybackParametersChanged(playbackParameters)

    override fun onSeekBackIncrementChanged(seekBackIncrementMs: Long) = to.onSeekBackIncrementChanged(seekBackIncrementMs)

    override fun onSeekForwardIncrementChanged(seekForwardIncrementMs: Long) = to.onSeekForwardIncrementChanged(seekForwardIncrementMs)

    override fun onMaxSeekToPreviousPositionChanged(maxSeekToPreviousPositionMs: Long) = to.onMaxSeekToPreviousPositionChanged(maxSeekToPreviousPositionMs)

    override fun onAudioSessionIdChanged(audioSessionId: Int) = to.onAudioSessionIdChanged(audioSessionId)

    override fun onAudioAttributesChanged(audioAttributes: AudioAttributes) = to.onAudioAttributesChanged(audioAttributes)

    override fun onVolumeChanged(volume: Float) = to.onVolumeChanged(volume)

    override fun onSkipSilenceEnabledChanged(skipSilenceEnabled: Boolean) = to.onSkipSilenceEnabledChanged(skipSilenceEnabled)

    override fun onDeviceInfoChanged(deviceInfo: DeviceInfo) = to.onDeviceInfoChanged(deviceInfo)

    override fun onDeviceVolumeChanged(volume: Int, muted: Boolean) = to.onDeviceVolumeChanged(volume, muted)

    override fun onVideoSizeChanged(videoSize: VideoSize) = to.onVideoSizeChanged(videoSize)

    override fun onSurfaceSizeChanged(width: Int, height: Int) = to.onSurfaceSizeChanged(width, height)

    override fun onRenderedFirstFrame() = to.onRenderedFirstFrame()

    override fun onCues(cues: MutableList<Cue>) = to.onCues(cues)

    override fun onCues(cueGroup: CueGroup) = to.onCues(cueGroup)

    override fun onMetadata(metadata: Metadata) = to.onMetadata(metadata)
}
