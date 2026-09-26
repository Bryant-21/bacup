Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == 1260
        BeginRadicalWalkOut()
        StartTimer(Utility.RandomFloat(WaitMin, WaitMax), iEVPPrepTimerID)
    ElseIf auiStageID == 115
        ; Nothing in the converted records sets 111 ("Player started scene with Crane"), so the
        ; Wake Crane objective never closed. The CraneAwakens phase (115) is that moment; the
        ; 0399a confrontation also sets 115 after Crane is already dead, hence the guards.
        If !IsStageDone(111) && !IsStageDone(300) && !IsStageDone(301) && !IsStageDone(399) && !IsStageDone(700)
            SetStage(111)
        EndIf
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == iWalkOutTimerID
        BeginRadicalWalkOut()
    ElseIf aiTimerID == iEVPPrepTimerID
        EvaluateRadicalWalkOut()
    EndIf
EndEvent

; Releases the Radicals towards the Wayward exit one at a time so they file out
; instead of leaving in a single block. Progress is derived from the
; RadicalsTravelToExit collection rather than a counter so a reload cannot
; restart or double-advance the sequence.
Function BeginRadicalWalkOut()
    If !IsRunning() || !IsStageDone(1260) || Radicals == None || RadicalsTravelToExit == None
        Return
    EndIf

    int radicalIndex = 0
    int radicalCount = Radicals.GetCount()
    While radicalIndex < radicalCount
        ObjectReference radicalRef = Radicals.GetAt(radicalIndex)
        If radicalRef != None && RadicalsTravelToExit.Find(radicalRef) < 0
            RadicalsTravelToExit.AddRef(radicalRef)
            Actor radicalActor = radicalRef as Actor
            If radicalActor != None
                radicalActor.EvaluatePackage()
            EndIf
            If radicalIndex + 1 < radicalCount
                StartTimer(Utility.RandomFloat(WalkoutUpdateTimeMin, WalkoutUpdateTimeMax), iWalkOutTimerID)
            EndIf
            Return
        EndIf
        radicalIndex += 1
    EndWhile
EndFunction

Function EvaluateRadicalWalkOut()
    If !IsRunning() || !IsStageDone(1260)
        Return
    EndIf
    If iEVPStage > 0 && !IsStageDone(iEVPStage)
        SetStage(iEVPStage)
    EndIf
    If Roper != None
        Actor roperRef = Roper.GetActorReference()
        If roperRef != None
            roperRef.EvaluatePackage()
        EndIf
    EndIf
    If Radicals != None
        Radicals.EvaluateAll()
    EndIf
    BeginRadicalWalkOut()
EndFunction

Event OnQuestShutdown()
    CancelTimer(iEVPPrepTimerID)
    CancelTimer(iWalkOutTimerID)
EndEvent
