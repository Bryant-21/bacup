; Alias 7 is the W05_MQ101P_BiometricScanner FURN, so the scanner "use" that
; drives stage 1200 arrives as a sit event on the player alias.
Event OnSit(ObjectReference akFurniture)
    If !akFurniture || !BiometricScanner
        Return
    EndIf
    If akFurniture != BiometricScanner.GetReference()
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(1100) && !owningQuest.IsStageDone(1200)
        owningQuest.SetStage(1200)
    EndIf
EndEvent
