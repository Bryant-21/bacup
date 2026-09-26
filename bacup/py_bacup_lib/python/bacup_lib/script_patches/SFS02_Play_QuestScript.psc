; Stage 150 ("player spoke to Chloe") and stages 610/1000 ("player activated Chloe") were
; authored to come from Chloe's dialogue and from scenes SFS02_Play_ChloeIntro /
; SFS02_Play_ChloeEnd. None of that survives conversion, so arrival at Chloe's trigger
; stands in for the opening conversation and walking back to her stands in for reporting in.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100 || auiStageID == 600
        StartTimer(5.0, 21)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 21 || !IsRunning()
        Return
    EndIf

    If IsStageDone(600)
        If !IsPlayerWithChloe()
            StartTimer(5.0, 21)
            Return
        EndIf
        If !IsStageDone(610)
            SetStage(610)
        EndIf
        If !IsStageDone(1000)
            SetStage(1000)
        EndIf
    ElseIf IsStageDone(100) && !IsStageDone(150)
        SetStage(150)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(21)
EndEvent

Bool Function IsPlayerWithChloe()
    ReferenceAlias chloeAlias = GetAlias(0) as ReferenceAlias
    ObjectReference chloeRef = None
    If chloeAlias != None
        chloeRef = chloeAlias.GetReference()
    EndIf

    Actor playerRef = None
    If SFS02Player != None
        playerRef = SFS02Player.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    ; Without a resolvable Chloe the daily could never be finished, so let it close out.
    If chloeRef == None || playerRef == None
        Return True
    EndIf
    Return playerRef.GetDistance(chloeRef) <= 400.0
EndFunction
