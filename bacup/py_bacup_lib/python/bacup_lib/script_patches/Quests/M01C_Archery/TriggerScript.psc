; ArcheryZone shooting line. Leaving it during the test shows the out-of-bounds warning (objective 40, whose
; B21:ObjectiveTimers countdown fails the test at 1100); stepping back in clears it.
Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    objectivesLock = False
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Alias_Player.GetReference()
        Return
    EndIf
    Actor player = akActionRef as Actor
    player.SetValue(M01C_Archery_IsInTrigger, 1.0)
    If TestRunning()
        OwningQuest.SetObjectiveDisplayed(40, False)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef != Alias_Player.GetReference()
        Return
    EndIf
    Actor player = akActionRef as Actor
    player.SetValue(M01C_Archery_IsInTrigger, 0.0)
    If TestRunning() && !objectivesLock
        objectivesLock = True
        OwningQuest.SetObjectiveDisplayed(40, True)
        If M01C_Archery_Warnings != None && !M01C_Archery_Warnings.IsPlaying()
            M01C_Archery_Warnings.Start()
        EndIf
        objectivesLock = False
    EndIf
EndEvent

Bool Function TestRunning()
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    Return OwningQuest.IsStageDone(110) && !OwningQuest.IsStageDone(1000) && !OwningQuest.IsStageDone(1100) && !OwningQuest.IsStageDone(9000)
EndFunction
