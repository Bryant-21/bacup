Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == 1260
        BeginRadicalWalkOut()
        StartTimer(Utility.RandomFloat(WaitMin, WaitMax), iEVPPrepTimerID)
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == iWalkOutTimerID
        BeginRadicalWalkOut()
    ElseIf aiTimerID == iEVPPrepTimerID
        If iEVPStage > 0 && !IsStageDone(iEVPStage)
            SetStage(iEVPStage)
        EndIf
    EndIf
EndEvent

; Releases the Radicals towards the Wayward exit one at a time so they file out
; instead of leaving in a single block. Progress is derived from the
; RadicalsTravelToExit collection rather than a counter so a reload cannot
; restart or double-advance the sequence.
Function BeginRadicalWalkOut()
    If Radicals == None || RadicalsTravelToExit == None
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
