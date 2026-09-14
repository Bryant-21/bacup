; Original method fill for FO76's server-side RadioGeneral_MasterScript stub.

Event OnInit()
    StartTimer(0.1, iFailSafeTimerID)
EndEvent

Event OnQuestInit()
    EnsureQueueRunning()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 10
        EnsureQueueRunning()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iFailSafeTimerID
        Return
    EndIf

    If lastScenePlayed != None && lastScenePlayed.IsPlaying()
        StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
        Return
    EndIf

    If lastScenePlayed != None
        UnregisterForRemoteEvent(lastScenePlayed, "OnEnd")
        lastScenePlayed = None
    EndIf
    QueueNextScene()
EndEvent

Function EnsureQueueRunning()
    If lastScenePlayed != None && lastScenePlayed.IsPlaying()
        Return
    EndIf

    CancelTimer(iFailSafeTimerID)
    If lastScenePlayed != None
        UnregisterForRemoteEvent(lastScenePlayed, "OnEnd")
        lastScenePlayed = None
    EndIf
    QueueNextScene()
EndFunction

Event Scene.OnEnd(Scene akSender)
    If akSender != lastScenePlayed
        Return
    EndIf

    UnregisterForRemoteEvent(akSender, "OnEnd")
    CancelTimer(iFailSafeTimerID)
    lastScenePlayed = None
    StartTimer(0.1, iFailSafeTimerID)
EndEvent

Function QueueNextScene()
    If lock_SceneQueue
        Return
    EndIf

    ; No IsRunning() gate: the quest reports not-running while the station is
    ; still starting up, so gating playback on it stalls the queue for a full
    ; failsafe interval on every load. RadioDramaRadio_MasterScript, the one
    ; station that has always played, has never had such a gate.
    lock_SceneQueue = True
    Scene nextScene = PickNextScene()
    If nextScene != None
        RegisterForRemoteEvent(nextScene, "OnEnd")
        nextScene.Start()
        ; Scene.Start() is asynchronous, so IsPlaying() is not reliable yet.
        ; OnTimer re-queues when the tracked scene stopped, which recovers a
        ; start that never took without stacking a second track over a live one.
        lastScenePlayed = nextScene
        songsPlayed += 1
    EndIf
    lock_SceneQueue = False
    StartTimer(fFailsafeTimerSeconds, iFailSafeTimerID)
EndFunction

Scene Function ResolveScene(Int index)
    If songFormIDs != None && index >= 0 && index < songFormIDs.Length && songFormIDs[index] != 0
        Scene resolvedScene = Game.GetFormFromFile(songFormIDs[index], "SeventySix.esm") as Scene
        If resolvedScene != None
            Return resolvedScene
        EndIf
    EndIf
    Return songsData[index].Track
EndFunction

Scene Function PickNextScene()
    If songsData == None || songsData.Length == 0
        Return None
    EndIf

    Int index = Utility.RandomInt(0, songsData.Length - 1)
    Int checked = 0
    Scene fallbackScene = None
    While checked < songsData.Length
        Scene candidate = ResolveScene(index)
        If candidate != None
            If fallbackScene == None
                fallbackScene = candidate
            EndIf
            If candidate != lastScenePlayed
                Return candidate
            EndIf
        EndIf

        index += 1
        If index >= songsData.Length
            index = 0
        EndIf
        checked += 1
    EndWhile
    Return fallbackScene
EndFunction
