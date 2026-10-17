; Plays the event's gramophone playlist from this marker in order, looping, while the event runs. Each song's
; songTimerID marks its end; IDTracker holds the playing sound instance so shutdown can stop it.
Event OnAliasInit()
    IDTracker = 0
    StartTimer(waitTimerDuration, 0)
EndEvent

Event OnTimer(Int aiTimerID)
    If BigBloom_Quest == None || !BigBloom_Quest.IsRunning() || BigBloom_Quest.IsStageDone(9000) || BigBloom_Quest.IsStageDone(9990)
        StopSong()
        Return
    EndIf
    If soundPlaylist == None || soundPlaylist.Length == 0
        Return
    EndIf
    Int nextSong = 0
    If aiTimerID != 0
        Int index = soundPlaylist.FindStruct("songTimerID", aiTimerID)
        If index >= 0
            nextSong = (index + 1) % soundPlaylist.Length
        EndIf
    EndIf
    PlaySong(nextSong)
EndEvent

Function PlaySong(Int aiIndex)
    ObjectReference marker = GetReference()
    soundData song = soundPlaylist[aiIndex]
    If marker == None || song == None || song.eventSong == None
        Return
    EndIf
    StopSong()
    IDTracker = song.eventSong.Play(marker)
    StartTimer(song.songDuration, song.songTimerID)
EndFunction

Function StopSong()
    If IDTracker > 0
        Sound.StopInstance(IDTracker)
    EndIf
    IDTracker = 0
EndFunction

Event OnAliasShutdown()
    StopSong()
EndEvent
