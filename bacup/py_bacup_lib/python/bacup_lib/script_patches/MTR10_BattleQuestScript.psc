Event OnQuestInit()
    ResetKeypads()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    RemoveAllInventoryEventFilters()
    If MTR10KeycardAlpha != None
        AddInventoryEventFilter(MTR10KeycardAlpha)
    EndIf
    If MTR10KeycardBeta != None
        AddInventoryEventFilter(MTR10KeycardBeta)
    EndIf
    RegisterForRemoteEvent(playerRef, "OnItemAdded")
EndEvent

Event OnQuestShutdown()
    CleanupBattle()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
    EndIf
    RemoveAllInventoryEventFilters()
EndEvent

Function SetGlobalValue(GlobalVariable akGlobal, Float afValue)
    If akGlobal != None
        akGlobal.SetValue(afValue)
    EndIf
EndFunction

Function ResetKeypads()
    CancelTimer(6101)
    AlphaActivated = 0
    BetaActivated = 0
    SetGlobalValue(MTR10_AlphaActivated, 0.0)
    SetGlobalValue(MTR10_BetaActivated, 0.0)
    SetGlobalValue(MTR10_KeypadsActive, 0.0)
    SetObjectiveDisplayed(182, False)
EndFunction

Float Function KeypadWindowSeconds()
    If MTR10_KeypadTimer != None && MTR10_KeypadTimer.GetValue() > 0.0
        Return MTR10_KeypadTimer.GetValue()
    EndIf
    Return 10.0
EndFunction

Function NotifyKeypadActivated(Bool abAlpha)
    If !GetStageDone(180) || GetStageDone(190) || GetStageDone(200)
        Return
    EndIf
    If abAlpha
        AlphaActivated = 1
        SetGlobalValue(MTR10_AlphaActivated, 1.0)
    Else
        BetaActivated = 1
        SetGlobalValue(MTR10_BetaActivated, 1.0)
    EndIf
    SetGlobalValue(MTR10_KeypadsActive, 1.0)

    If AlphaActivated > 0 && BetaActivated > 0
        CancelTimer(6101)
        SetObjectiveDisplayed(182, False)
        SetStage(190)
        Return
    EndIf

    ; Only one panel is live, so FO76 gave the second one MTR10_KeypadTimer seconds
    ; before both panels reset. Objective 182 points at whichever panel is still dark.
    SetObjectiveDisplayed(182, True, True)
    CancelTimer(6101)
    StartTimer(KeypadWindowSeconds(), 6101)
EndFunction

Function GiveKeycard(ReferenceAlias akGutsyAlias, Key akKeycard)
    If akGutsyAlias == None || akKeycard == None
        Return
    EndIf
    ObjectReference gutsyRef = akGutsyAlias.GetReference()
    If gutsyRef == None || gutsyRef.GetItemCount(akKeycard) > 0
        Return
    EndIf
    gutsyRef.AddItem(akKeycard, 1, True)
EndFunction

Function StockLootContainer(ReferenceAlias akContainerAlias, LeveledItem akLootList)
    If akContainerAlias == None || akLootList == None
        Return
    EndIf
    ObjectReference containerRef = akContainerAlias.GetReference()
    If containerRef == None
        Return
    EndIf
    containerRef.AddItem(akLootList, 1, True)
EndFunction

Function OpenBunker(ReferenceAlias akDoorAlias)
    ObjectReference doorRef = None
    If akDoorAlias != None
        doorRef = akDoorAlias.GetReference()
    EndIf
    ; The bunker door is the one-shot: stage 160 has three producers (stage 150, the
    ; BunkerTrigger alias and the retreat watch) and the quest allows repeated stages.
    ; GetOpenState: 1 open, 2 opening, 3 closed, 4 closing.
    Int doorState = 0
    If doorRef != None
        doorState = doorRef.GetOpenState()
    EndIf
    If doorState == 1 || doorState == 2
        Return
    EndIf
    If doorRef != None
        If doorRef.IsLocked()
            doorRef.Lock(False)
        EndIf
        doorRef.SetOpen(True)
    EndIf

    GiveKeycard(MrGutsyA, MTR10KeycardAlpha)
    GiveKeycard(MrGutsyB, MTR10KeycardBeta)
    ; LootContainer already receives LootList01-03 through its own alias inventory.
    StockLootContainer(LootContainer02, LootList02)
    StockLootContainer(LootContainer03, LootList03)
EndFunction

Function TrackKeycard(RefCollectionAlias akCollection, ObjectReference akItemReference)
    If akCollection == None || akItemReference == None
        Return
    EndIf
    If akCollection.Find(akItemReference) < 0
        akCollection.AddRef(akItemReference)
    EndIf
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer() || !GetStageDone(160) || GetStageDone(180)
        Return
    EndIf
    ; FO76 aliased the keycards through fill-less aliases, so the pickup stages are
    ; driven from the player's inventory here and the refs join the cleanup collections.
    If MTR10KeycardAlpha != None && akBaseItem == MTR10KeycardAlpha
        TrackKeycard(KeyCardAlpha, akItemReference)
        If !GetStageDone(170)
            SetStage(170)
        EndIf
    ElseIf MTR10KeycardBeta != None && akBaseItem == MTR10KeycardBeta
        TrackKeycard(KeyCardBeta, akItemReference)
        If !GetStageDone(171)
            SetStage(171)
        EndIf
    EndIf
EndEvent

Function BeginRetreatWatch()
    CancelTimer(6102)
    StartTimer(5.0, 6102)
EndFunction

ObjectReference Function BunkerDoorRef()
    ReferenceAlias doorAlias = GetAlias(3) as ReferenceAlias
    If doorAlias == None
        Return None
    EndIf
    Return doorAlias.GetReference()
EndFunction

Function FinishBattle()
    B21:QuestTimer questTimer = (Self as Quest) as B21:QuestTimer
    If questTimer != None
        ; FO76 flagged stages 300 and 400 TimerEnd, and B21:QuestTimer sets both, so a
        ; finished run must not leave the timer armed or it would fail the completed quest.
        questTimer.StopQuestTimer()
    EndIf
    CancelTimer(6101)
    CancelTimer(6102)
    CancelTimer(6103)
    ; Let the completion reward rows and the quest notification land before shutdown.
    StartTimer(5.0, 6103)
EndFunction

Function CleanupBattle()
    CancelTimer(6101)
    CancelTimer(6102)
    CancelTimer(6103)
    AlphaActivated = 0
    BetaActivated = 0
    SetGlobalValue(MTR10_AlphaActivated, 0.0)
    SetGlobalValue(MTR10_BetaActivated, 0.0)
    SetGlobalValue(MTR10_KeypadsActive, 0.0)
EndFunction

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If aiTimerID == 6101
        If AlphaActivated > 0 && BetaActivated > 0
            SetStage(190)
        Else
            ResetKeypads()
        EndIf
    ElseIf aiTimerID == 6102
        Actor robotActor = None
        If Robot != None
            robotActor = Robot.GetActorReference()
        EndIf
        ObjectReference doorRef = BunkerDoorRef()
        If GetStageDone(160) || GetStageDone(200)
            Return
        EndIf
        ; The retreating bot leads the player home; its arrival, or its death on the way,
        ; opens the bunker. FO76 did this from the RETREAT package's end fragment.
        If robotActor == None || robotActor.IsDead()
            SetStage(160)
        ElseIf doorRef != None && robotActor.GetDistance(doorRef) <= 512.0
            SetStage(160)
        Else
            StartTimer(5.0, 6102)
        EndIf
    ElseIf aiTimerID == 6103
        If !GetStageDone(500)
            SetStage(500)
        EndIf
    EndIf
EndEvent
