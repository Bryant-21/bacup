Event OnAliasInit()
    AddInventoryEventFilter(NukeCodePage)
    RestoreOfficer()
EndEvent

Event OnDeath(Actor akKiller)
    StartTimer(Utility.RandomFloat(respawnTimeMin, respawnTimeMax), respawnTimerID)
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If akBaseItem == NukeCodePage
        Nuke_CodesOfficerRefScript officer = GetReference() as Nuke_CodesOfficerRefScript
        If officer != None
            officer.StopBeepingRMI()
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == respawnTimerID
        RestoreOfficer()
    EndIf
EndEvent

Function RestoreOfficer()
    Actor officer = GetReference() as Actor
    If officer == None
        Return
    EndIf
    If officer.IsDead()
        officer.Reset()
    EndIf
    If officer.GetItemCount(NukeCodePage) == 0
        officer.AddItem(NukeCodePage, 1, True)
    EndIf
EndFunction
