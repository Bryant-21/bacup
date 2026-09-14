; Original method fill for FO76's server-side RadioDramaRadio_MasterScript stub.
; Pirate Radio interleaves songs, commercials and radio dramas; the declared
; trackTypeOrder / currOrderIndex drive that rotation.

Event OnInit()
    BuildTrackTypeOrder()
    StartTimer(0.1, iFailSafeTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iFailSafeTimerID
        Return
    EndIf

    Scene playing = LastScenePlayedForCurrentType()
    If playing != None && playing.IsPlaying()
        StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
        Return
    EndIf

    ClearLastScenePlayed()
    QueueNextScene()
EndEvent

Event Scene.OnEnd(Scene akSender)
    If akSender != LastScenePlayedForCurrentType()
        Return
    EndIf

    UnregisterForRemoteEvent(akSender, "OnEnd")
    CancelTimer(iFailSafeTimerID)
    ClearLastScenePlayed()
    isScenePlaying = False
    AdvanceTrackType()
    StartTimer(0.1, iFailSafeTimerID)
EndEvent

Function BuildTrackTypeOrder()
    ; Two songs, then a drama, then a commercial — keeps spoken tracks spaced.
    trackTypeOrder = New Int[4]
    trackTypeOrder[0] = TRACK_TYPE_SONG
    trackTypeOrder[1] = TRACK_TYPE_SONG
    trackTypeOrder[2] = TRACK_TYPE_DRAMA
    trackTypeOrder[3] = TRACK_TYPE_COMMERCIAL
    currOrderIndex = 0
EndFunction

Int Function CurrentTrackType()
    If trackTypeOrder == None || trackTypeOrder.Length == 0
        Return TRACK_TYPE_SONG
    EndIf
    Return trackTypeOrder[currOrderIndex % trackTypeOrder.Length]
EndFunction

Function AdvanceTrackType()
    If trackTypeOrder == None || trackTypeOrder.Length == 0
        Return
    EndIf
    currOrderIndex += 1
    If currOrderIndex >= trackTypeOrder.Length
        currOrderIndex = 0
    EndIf
EndFunction

; CurrentTrackType() is compared inline rather than stored in a local: the
; native compiler mis-types same-script helper returns in assignment position
; for value types ("cannot assign None to Int").
Scene Function LastScenePlayedForCurrentType()
    If CurrentTrackType() == TRACK_TYPE_DRAMA
        Return lastDramaScenePlayed
    ElseIf CurrentTrackType() == TRACK_TYPE_COMMERCIAL
        Return lastCommercialScenePlayed
    EndIf
    Return lastSongScenePlayed
EndFunction

Function ClearLastScenePlayed()
    Scene playing = LastScenePlayedForCurrentType()
    If playing == None
        Return
    EndIf
    UnregisterForRemoteEvent(playing, "OnEnd")
    If CurrentTrackType() == TRACK_TYPE_DRAMA
        lastDramaScenePlayed = None
    ElseIf CurrentTrackType() == TRACK_TYPE_COMMERCIAL
        lastCommercialScenePlayed = None
    Else
        lastSongScenePlayed = None
    EndIf
EndFunction

Function RememberScene(Scene playing)
    If CurrentTrackType() == TRACK_TYPE_DRAMA
        lastDramaScenePlayed = playing
    ElseIf CurrentTrackType() == TRACK_TYPE_COMMERCIAL
        lastCommercialScenePlayed = playing
    Else
        lastSongScenePlayed = playing
    EndIf
EndFunction

Function QueueNextScene()
    If isScenePlaying
        Return
    EndIf

    isScenePlaying = True
    Scene nextScene = PickNextScene()
    If nextScene != None
        RememberScene(nextScene)
        RegisterForRemoteEvent(nextScene, "OnEnd")
        nextScene.Start()
    Else
        ; Nothing playable for this slot — do not stall the rotation on it.
        isScenePlaying = False
        AdvanceTrackType()
    EndIf
    StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
EndFunction

Scene Function ResolveScene(tracksDatum[] tracks, Int[] formIDs, Int index)
    If formIDs != None && index >= 0 && index < formIDs.Length && formIDs[index] != 0
        Scene resolvedScene = Game.GetFormFromFile(formIDs[index], "SeventySix.esm") as Scene
        If resolvedScene != None
            Return resolvedScene
        EndIf
    EndIf
    Return tracks[index].trackScene
EndFunction

Scene Function PickFrom(tracksDatum[] tracks, Int[] formIDs, Scene lastPlayed)
    If tracks == None || tracks.Length == 0
        Return None
    EndIf

    Int index = Utility.RandomInt(0, tracks.Length - 1)
    Int checked = 0
    Scene fallbackScene = None
    While checked < tracks.Length
        Scene candidate = ResolveScene(tracks, formIDs, index)
        If candidate != None
            If fallbackScene == None
                fallbackScene = candidate
            EndIf
            If candidate != lastPlayed
                Return candidate
            EndIf
        EndIf

        index += 1
        If index >= tracks.Length
            index = 0
        EndIf
        checked += 1
    EndWhile
    Return fallbackScene
EndFunction

Scene Function PickNextScene()
    If CurrentTrackType() == TRACK_TYPE_DRAMA
        Return PickFrom(dramasData, dramaFormIDs, lastDramaScenePlayed)
    ElseIf CurrentTrackType() == TRACK_TYPE_COMMERCIAL
        Return PickFrom(commercialsData, commercialFormIDs, lastCommercialScenePlayed)
    EndIf
    Return PickFrom(songsData, songFormIDs, lastSongScenePlayed)
EndFunction
