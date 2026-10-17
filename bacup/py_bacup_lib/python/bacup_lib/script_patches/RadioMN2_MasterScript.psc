; Mischief Night radio: while the season is live (MN2_LCP_Global when bUseLCPStatus), the transmitter plays the
; MusicScenes one after another in random order without an immediate repeat. A failsafe timer restarts the rotation
; if a scene never reports OnEnd (shorter while the Mischief Night event runs).
Event OnQuestInit()
    lastPlayedMusicIndex = -1
    StartTimer(1.0, iFailSafeTimerID)
EndEvent

Function PlayNextScene()
    If MusicScenes == None || MusicScenes.Length == 0
        Return
    EndIf
    If bUseLCPStatus && MN2_LCP_Global != None && MN2_LCP_Global.GetValue() <= 0.0
        StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
        Return
    EndIf
    Int index = Utility.RandomInt(0, MusicScenes.Length - 1)
    If MusicScenes.Length > 1 && index == lastPlayedMusicIndex
        index = (index + 1) % MusicScenes.Length
    EndIf
    lastPlayedMusicIndex = index
    currentlyPlaying = MusicScenes[index]
    If currentlyPlaying != None
        RegisterForRemoteEvent(currentlyPlaying, "OnEnd")
        currentlyPlaying.Start()
    EndIf
    Float failsafe = fFailsafeTimerSeconds
    If MN2_Quest_Mischief != None && MN2_Quest_Mischief.IsRunning()
        failsafe = fFailsafeTimerSecondsEventActive
    EndIf
    StartTimer(failsafe, iFailSafeTimerID)
EndFunction

Event Scene.OnEnd(Scene akSender)
    UnregisterForRemoteEvent(akSender, "OnEnd")
    If akSender == currentlyPlaying
        currentlyPlaying = None
        PlayNextScene()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iFailSafeTimerID
        Return
    EndIf
    If currentlyPlaying == None || !currentlyPlaying.IsPlaying()
        PlayNextScene()
    Else
        StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
    EndIf
EndEvent
